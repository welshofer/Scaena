//! A deck made from a prompt for Swift (PLAN G.6, ADR-0026): what `scaena generate` does, a step
//! at a time, the client making each request with `URLSession` and handing each answer back. The
//! sources are read here from their bytes, each link planned as GETs (`scaena_sources::fetch`:
//! SEC, with the user's contact, and Google files shared by link, reached as the CLI reaches
//! them), deep research polled or resumed, Narrato's arc and slides asked, the slides' pictures
//! made by the user's image model (PLAN G.10, every one at once), the deck a narrative builds
//! made in a theme that ships, and the bundle made as the CLI makes it
//! (`scaena_session::making`). The key a step is handed builds that step's request and is kept
//! nowhere here.

use crate::{Failure, said};
use base64::Engine;
use scaena_narrato::Picture;
use scaena_narrato::fit::Long;
use scaena_narrato::look::Seen;
use scaena_narrato::research::{Asked, Next, Report, Research, source_name};
use scaena_narrato::{Brief, Document, Generation, Provider, Request, Settings, Step};
use scaena_ops::narrative::{Asked as Wanted, Painting, Pictures, Reply, look_rounds};
use scaena_ops::scripts::Script;
use scaena_session::Session;
use scaena_session::making::Making;
use scaena_sources::fetch::{Fetch, Get, Reach, Step as Fetched};
use scaena_sources::{Given, Kind, Source};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A deck being made from a prompt: the sources read so far, the link being fetched, the
/// research under way, the narrative being written, and the bundle being made of it.
/// A look at the deck made, in rounds (PLAN G.13, narrative PERF-5): each slide as the model
/// is shown it and as drawn, and the rounds asked, every one at once, which the client sends
/// `LOOKS_AT_ONCE` at a time, as the CLI and the browser do (lift-up 20261025 PERF-2).
#[derive(Default)]
struct Look {
    seen: Vec<Seen>,
    drawn: Vec<Picture>,
    asked: Vec<std::ops::Range<usize>>,
}

impl Look {
    /// Every round's request, in order, with how many slides the look shows in all and how many
    /// rounds the client asks at once; none where no round asks anything.
    fn rounds(&mut self, generation: &Generation, key: &str) -> Option<Value> {
        let mut requests = Vec::new();
        for round in look_rounds(self.seen.len()) {
            let edges = [round.start.checked_sub(1).and_then(|b| self.seen.get(b)), self.seen.get(round.end)];
            if let Some(asked) =
                generation.look_request(key, &self.seen[round.clone()], &self.drawn[round.clone()], edges)
            {
                self.asked.push(round);
                requests.push(request(asked));
            }
        }
        (!requests.is_empty()).then(
            || json!({ "rounds": requests, "slides": self.seen.len(), "atOnce": scaena_ops::narrative::LOOKS_AT_ONCE }),
        )
    }
}

#[derive(Default)]
pub struct Generating {
    sources: Vec<Source>,
    fetching: Option<Fetch>,
    research: Option<(Research, String)>,
    generation: Option<Written>,
    /// The pictures the slides ask for, once asked: being made, and this round's answers.
    painting: Option<(Painting, Vec<Reply>)>,
    /// The pictures made: where each slide's is kept, their bytes, and why each other was not.
    painted: Option<Painted>,
    making: Option<(Making, Vec<Long>)>,
    /// How many times the model has been asked to rewrite what runs past its place (STAB-4).
    fit_rounds: usize,
    /// What the model is shown of the deck made, while it is asked what it would change on it
    /// (PLAN G.13), a dozen slides a round (narrative PERF-5), and the theme it is made in.
    look: Look,
    theme: String,
    /// The font for the script the deck is written in, where the theme's fonts do not draw it,
    /// once Swift has fetched it and its digest is the one pinned (PLAN G.22).
    script: Option<(&'static Script, Vec<u8>)>,
    /// A map being fetched from Geoapify (ADR-0030, PLAN O.1d), and the map once it is.
    map: Option<scaena_objects::map::MapPlan>,
}

/// A [`Generating`] goes from one thread to another between steps (lift-up narrative PERF-3): the
/// Mac and the iPad make the steps that build the deck and draw its slides away from the main
/// actor, one step at a time.
const _: fn() = || {
    fn sent<T: Send>() {}
    sent::<Generating>();
};

/// The pictures made (`Painting::finish`): where each slide's is kept, their bytes by their paths
/// in the bundle, and why each other asked for was not.
type Painted = (Pictures, BTreeMap<String, Vec<u8>>, Vec<String>);

/// What a brief may ask, as every client offers it (`scaena_narrato::brief::choices`), and the
/// themes that ship, by family, to make the deck in (PLAN T.0d).
pub fn choices() -> Value {
    let mut choices = scaena_narrato::brief::choices();
    if let Some(choices) = choices.as_object_mut() {
        choices.insert("themes".into(), crate::shipped_themes());
    }
    choices
}

/// The narrative being written, the PDFs handed to the model beside its prompt, and what each
/// source gave the prompt.
struct Written {
    generation: Generation,
    documents: Vec<Document>,
    given: Vec<Given>,
}

/// A request as Swift makes it (`ProviderRequest`): its body the text to send.
fn request(r: Request) -> Value {
    json!({ "method": r.method, "url": r.url, "headers": r.headers, "body": r.body.map(|b| b.to_string()) })
}

/// A GET a map's plan asks for, as a request, `key` in the slot the plan names: what Swift sends
/// and never writes down.
fn keyed(g: &scaena_objects::Get, key: &str) -> Value {
    let headers: serde_json::Map<String, Value> =
        g.keyed_headers(key).into_iter().map(|(k, v)| (k, Value::String(v))).collect();
    json!({ "method": "GET", "url": g.keyed_url(key), "headers": headers, "most": g.most })
}

/// A GET a link's plan asks for, as a request, with the most bytes of its answer to read.
fn get(g: Get) -> Value {
    let headers: serde_json::Map<String, Value> = g.headers.into_iter().map(|(k, v)| (k, Value::String(v))).collect();
    json!({ "method": "GET", "url": g.url, "headers": headers, "most": g.most })
}

fn provider(name: &str) -> Result<Provider, Failure> {
    Provider::named(name).ok_or_else(|| said(format!("`{name}` is no provider: anthropic, openai, or gemini")))
}

/// The brief `args` hold, Narrato's as JSON.
fn brief(args: &Value) -> Result<Brief, Failure> {
    serde_json::from_value(args.get("brief").cloned().unwrap_or(Value::Null))
        .map_err(|e| said(format!("the brief: {e}")))
}

/// Who writes it: `{provider, model, base?}`.
fn asked(args: &Value) -> Result<(Provider, String, Option<String>), Failure> {
    let name = args.get("provider").and_then(Value::as_str).ok_or_else(|| said("`provider` is a string it needs"))?;
    let model = args.get("model").and_then(Value::as_str).unwrap_or_default().to_string();
    let base = args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty()).map(str::to_string);
    Ok((provider(name)?, model, base))
}

/// What a source is, said: `{name, kind}`.
fn read(source: &Source) -> Value {
    json!({ "name": source.name, "kind": source.kind.name() })
}

impl Generating {
    /// The JSON of the theme that ships as `shipped`, with the family of the script's font where
    /// one was given (PLAN G.22).
    fn theme_text(&self, shipped: &scaena_ops::shipped::Shipped) -> Result<String, Failure> {
        match &self.script {
            Some((script, _)) => scaena_ops::scripts::taken(shipped.text, script).map_err(said),
            None => Ok(shipped.text.to_string()),
        }
    }

    /// A font the theme names, by its path: the script's, where one was given, else one that
    /// ships.
    fn font(&self, shipped: &scaena_ops::shipped::Shipped, file: &str) -> Result<Vec<u8>, scaena_ops::OpsError> {
        match &self.script {
            Some((script, bytes)) if script.file == file => Ok(bytes.clone()),
            _ => shipped_font(shipped, file),
        }
    }

    /// One step, by its name, with `bytes` where a step reads some: `add` (a file's),
    /// `fetched` (a GET's answer's), and `scriptFont` (the script's font's).
    pub fn call(&mut self, method: &str, args: &Value, bytes: &[u8]) -> Result<Value, Failure> {
        let arg = |key: &str| {
            args.get(key).and_then(Value::as_str).ok_or_else(|| said(format!("{method}: `{key}` is a string it needs")))
        };
        let status = || args.get("status").and_then(Value::as_u64).unwrap_or(0) as u16;
        let status_text = || args.get("statusText").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(match method {
            "add" => {
                let source = scaena_sources::read(arg("name")?, bytes).map_err(said)?;
                self.keep(source)?
            }
            "fetch" => {
                let spec = arg("spec")?;
                let remote = scaena_sources::fetch::remote(spec)
                    .map_err(said)?
                    .ok_or_else(|| said(format!("{spec}: a file is added as a file, not as a link")))?;
                let text = |k: &str| args.get(k).and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
                let reach = Reach { contact: text("contact"), token: text("token"), browser: false };
                let (fetch, first) = Fetch::new(remote, &reach).map_err(said)?;
                self.fetching = Some(fetch);
                json!({ "get": get(first) })
            }
            "fetched" => {
                let fetch = self.fetching.as_mut().ok_or_else(|| said("fetched: no link is being fetched"))?;
                match fetch.answer(status(), args.get("kind").and_then(Value::as_str).unwrap_or_default(), bytes) {
                    Ok(Fetched::Get(next)) => json!({ "get": get(next) }),
                    Ok(Fetched::Done(source)) => {
                        self.fetching = None;
                        json!({ "read": self.keep(source)? })
                    }
                    Err(e) => {
                        self.fetching = None;
                        return Err(said(e));
                    }
                }
            }
            "research" => {
                let filled = brief(args)?.validated().map_err(said)?;
                let topic = filled.topic.clone();
                let (provider, model, base) = asked(args)?;
                let asking =
                    Asked { topic: filled.topic, audience: filled.audience, goal: filled.goal, length: filled.length };
                let research = Research::new(&asking, provider, &model, base.as_deref(), false).map_err(said)?;
                let first = research.start(arg("key")?);
                self.research = Some((research, topic));
                json!({ "send": request(first) })
            }
            "researched" => {
                let (research, topic) =
                    self.research.as_mut().ok_or_else(|| said("researched: no research is under way"))?;
                let next = research.answer(arg("key")?, status(), &status_text(), arg("body")?).map_err(said)?;
                let progress = research.progress.clone();
                match next {
                    Next::Send(r) => json!({ "send": request(r), "progress": progress }),
                    Next::Wait { ms, then } => json!({ "wait": ms, "send": request(then), "progress": progress }),
                    Next::Done(report) => {
                        let done = json!({ "sources": report.sources.len(), "cut": report.cut });
                        let source = researched(topic, &report);
                        self.research = None;
                        self.keep(source)?;
                        json!({ "done": done, "progress": progress })
                    }
                }
            }
            "script" => {
                // The script whose font a deck in the brief's language takes beside the theme's
                // (PLAN G.22): where Swift fetches it, and the digest it must have; or none.
                let shipped = shipped(arg("theme")?)?;
                let language = args.get("language").and_then(Value::as_str).filter(|l| !l.is_empty());
                let script =
                    scaena_ops::scripts::needed(language, "", shipped.text, &|file| shipped_font(&shipped, file))
                        .map_err(said)?;
                let base = args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty());
                json!({ "script": script.map(|s| json!({
                    "key": s.key, "family": s.family, "language": s.language, "url": s.url(base),
                    "sha256": s.sha256, "bytes": s.bytes,
                })) })
            }
            "scriptFont" => {
                // The font Swift fetched for the script `key` names, kept once it is the one pinned.
                let key = arg("key")?;
                let script = Script::keyed(key).ok_or_else(|| said(format!("`{key}` is no script's")))?;
                script.check(bytes).map_err(said)?;
                self.script = Some((script, bytes.to_vec()));
                json!({})
            }
            "writable" => {
                // Whether the theme's fonts, with the script's where one was given, draw the brief's
                // language, asked before any call is paid for: a deck in letters its fonts cannot
                // draw would be blank (PLAN G.21, G.22).
                let shipped = shipped(arg("theme")?)?;
                let theme = self.theme_text(&shipped)?;
                let language = args.get("language").and_then(Value::as_str).filter(|l| !l.is_empty());
                (scaena_ops::narrative::writable(language, &theme, true, &|file| self.font(&shipped, file)))
                    .map_err(said)?;
                json!({})
            }
            "begin" => {
                let mut brief = brief(args)?;
                let (provider, model, base) = asked(args)?;
                let ground =
                    scaena_sources::ground(&self.sources, provider.document_bytes(), provider.name()).map_err(said)?;
                brief.source_content = ground.content;
                let documents = (ground.pdfs.iter())
                    .map(|(name, pdf)| Document {
                        name: name.clone(),
                        media_type: "application/pdf".into(),
                        data: base64::engine::general_purpose::STANDARD.encode(pdf),
                    })
                    .collect();
                let settings = Settings { provider, model, base, browser: false };
                let mut generation = Generation::new(brief, settings).map_err(said)?;
                if let Some(arc) = args.get("arc").and_then(Value::as_str).filter(|a| !a.is_empty()) {
                    generation.choose_arc(Some(arc));
                }
                let given = ground.given;
                let said_given = json!({ "given": given });
                self.generation = Some(Written { generation, documents, given });
                said_given
            }
            "request" => {
                let w = self.written()?;
                let step = w.generation.step();
                match w.generation.request(arg("key")?, &w.documents) {
                    Some(r) => json!({ "step": step, "slides": w.generation.target(), "request": request(r) }),
                    None => json!({ "step": step }),
                }
            }
            "answer" => {
                let w = self.written()?;
                let after = args.get("after").and_then(Value::as_str);
                w.generation.answer(status(), &status_text(), arg("body")?, after).map_err(said)?;
                where_it_stands(&w.generation)
            }
            "written" => {
                let w = self.written()?;
                w.generation.written(arg("text")?, false).map_err(said)?;
                where_it_stands(&w.generation)
            }
            // The outline before the slides (narrative FUNC-1): asked once the arc is chosen, the
            // next `request` the outline's; `settle` takes it as the person settled it, and
            // `chooseArc` another arc, by name or the heuristic's, to outline again.
            "outline" => {
                let w = self.written()?;
                w.generation.outline_first().map_err(said)?;
                where_it_stands(&w.generation)
            }
            "settle" => {
                let w = self.written()?;
                let outline: scaena_narrato::Outline =
                    serde_json::from_value(args.get("outline").cloned().unwrap_or(Value::Null))
                        .map_err(|e| said(format!("settle: `outline`: {e}")))?;
                w.generation.settle(&outline).map_err(said)?;
                where_it_stands(&w.generation)
            }
            "chooseArc" => {
                let w = self.written()?;
                let arc = args.get("arc").and_then(Value::as_str).filter(|a| !a.is_empty());
                w.generation.choose_arc(arc);
                where_it_stands(&w.generation)
            }
            "imageModels" => {
                let provider = provider(arg("provider")?)?;
                let base = args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty());
                json!({ "request": request(provider.models(arg("key")?, base, false)) })
            }
            "imageModelsRead" => {
                let provider = provider(arg("provider")?)?;
                let models =
                    scaena_narrato::pictures::models(provider, status(), &status_text(), arg("body")?).map_err(said)?;
                json!({ "models": models })
            }
            "pictures" => {
                // The pictures the slides ask for in the theme, made as `look`, by `provider`'s
                // `model`: `{pictures}`, how many.
                let theme = shipped(arg("theme")?)?;
                let look = scaena_narrato::pictures::Look::named(arg("look")?)
                    .ok_or_else(|| said("pictures: `look` is illustration or photo"))?;
                let w = self.written()?;
                let narrative =
                    w.generation.narrative().ok_or_else(|| said("pictures: the narrative is not written yet"))?;
                let model: scaena_core::model::theme::Theme =
                    serde_json::from_str(theme.text).map_err(|e| said(format!("the theme: {e}")))?;
                let wanted: Vec<Wanted> = scaena_ops::narrative::asking(narrative, &model, look);
                let settings = scaena_narrato::pictures::Settings {
                    provider: provider(arg("provider")?)?,
                    model: arg("model")?.to_string(),
                    base: args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty()).map(str::to_string),
                    quality: (args.get("quality").and_then(Value::as_str))
                        .and_then(scaena_narrato::pictures::Quality::named)
                        .unwrap_or_default(),
                };
                let n = wanted.len();
                self.painting = Some((Painting::new(wanted, settings), Vec::new()));
                self.painted = None;
                json!({ "pictures": n })
            }
            "paint" => {
                // This round's requests, `{requests}`: none once each picture is made or given up.
                let (painting, replies) = self.painting.as_mut().ok_or_else(|| said("paint: no pictures are asked"))?;
                replies.clear();
                let requests: Vec<Value> = painting.requests(arg("key")?).into_iter().map(request).collect();
                json!({ "requests": requests })
            }
            "painted" => {
                // An answer to this round's next request, its body the bytes handed over.
                let (_, replies) = self.painting.as_mut().ok_or_else(|| said("painted: no pictures are asked"))?;
                let after = args.get("after").and_then(Value::as_str).map(str::to_string);
                let body = String::from_utf8_lossy(bytes).into_owned();
                replies.push(Reply { status: status(), status_text: status_text(), body, after });
                json!({})
            }
            "round" => {
                // The round's answers read: `{wait}`, the seconds to wait before the next round.
                let (painting, replies) = self.painting.as_mut().ok_or_else(|| said("round: no pictures are asked"))?;
                json!({ "wait": painting.answer(std::mem::take(replies)) })
            }
            "finish" => {
                // The pictures made, kept for `make`: `{pictures, unmade}`.
                let (painting, _) = self.painting.take().ok_or_else(|| said("finish: no pictures are asked"))?;
                let (placed, assets, unmade) = painting.finish();
                let out = json!({ "pictures": placed.len(), "unmade": unmade });
                self.painted = Some((placed, assets, unmade));
                out
            }
            "picture" => {
                // One picture a person asks for in an editor (PLAN 2.107): of `subject`, made as
                // `look` in the palette of `theme` (the deck's theme, its JSON), for a place
                // `width` × `height` on the slide, by `provider`'s `model`; painted by `paint`,
                // `painted`, and `round` as a made deck's are: `{pictures}`, one.
                let theme: scaena_core::model::theme::Theme =
                    serde_json::from_str(arg("theme")?).map_err(|e| said(format!("the theme: {e}")))?;
                let look = scaena_narrato::pictures::Look::named(arg("look")?)
                    .ok_or_else(|| said("picture: `look` is illustration or photo"))?;
                let subject = arg("subject")?;
                if subject.trim().is_empty() {
                    return Err(said("say what the picture shows"));
                }
                let size = |key: &str| {
                    args.get(key)
                        .and_then(Value::as_f64)
                        .filter(|v| *v > 0.0)
                        .ok_or_else(|| said(format!("picture: `{key}`")))
                };
                let asked = scaena_ops::narrative::asked_one(&theme, subject, look, size("width")?, size("height")?);
                let settings = scaena_narrato::pictures::Settings {
                    provider: provider(arg("provider")?)?,
                    model: arg("model")?.to_string(),
                    base: args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty()).map(str::to_string),
                    quality: (args.get("quality").and_then(Value::as_str))
                        .and_then(scaena_narrato::pictures::Quality::named)
                        .unwrap_or_default(),
                };
                self.painting = Some((Painting::new(vec![asked], settings), Vec::new()));
                self.painted = None;
                json!({ "pictures": 1 })
            }
            "pictureMade" => {
                // The picture `picture` asked for, made: `{bytes, ext, alt}`, its bytes in base64,
                // its file's kind (`png`, `jpg`), and what it shows; or why none was made.
                let (painting, _) = self.painting.take().ok_or_else(|| said("pictureMade: no picture is asked"))?;
                let (placed, assets, unmade) = painting.finish();
                let made = placed.values().next().and_then(|p| Some((p, assets.get(&p.src)?)));
                let Some((put, bytes)) = made else {
                    let why = unmade.join("; ");
                    return Err(said(if why.is_empty() { "no picture was made".to_string() } else { why }));
                };
                let ext = put.src.rsplit_once('.').map_or("png", |(_, ext)| ext);
                json!({ "bytes": base64::engine::general_purpose::STANDARD.encode(bytes), "ext": ext, "alt": put.alt })
            }
            "map" => {
                // A map of a place a person names (ADR-0030, PLAN O.1d): `ask` as the browser's
                // sheet asks (`scaena_objects::map::Asked`), `key` the user's Geoapify key, which
                // goes in the request's query alone: `{request}`, the first GET to make.
                let ask: scaena_objects::map::Asked =
                    serde_json::from_value(args.get("ask").cloned().unwrap_or(Value::Null))
                        .map_err(|e| said(format!("map: `ask`: {e}")))?;
                let key = arg("key")?;
                if key.trim().is_empty() {
                    return Err(said("map: no Geoapify key: keep one in Settings"));
                }
                let (plan, first) = scaena_objects::map::MapPlan::new(ask.ask(false).map_err(said)?).map_err(said)?;
                self.map = Some(plan);
                json!({ "request": keyed(&first, key) })
            }
            "mapAnswered" => {
                // The answer to the last GET `map` asked for, its body `bytes`, `status` and `kind`
                // its status and content type, `at` when it came (RFC 3339, UTC, by Swift's clock):
                // `{request}`, the next; `{choose: [place]}`, the places a name matched, to ask
                // again with a `pick`; or `{map: {fetched, pin, place, png}}`, the picture in
                // base64, for the session's `mapping`.
                use scaena_objects::{Answer, Plan, Step};
                let plan = self.map.as_mut().ok_or_else(|| said("mapAnswered: no map is asked: `map` first"))?;
                let status = args.get("status").and_then(Value::as_u64).ok_or_else(|| said("mapAnswered: `status`"))?;
                let answer = Answer {
                    status: u16::try_from(status).map_err(|_| said("mapAnswered: `status`"))?,
                    kind: args.get("kind").and_then(Value::as_str).unwrap_or_default().to_string(),
                    body: bytes.to_vec(),
                };
                let step = plan.answer(&answer).map_err(|e| {
                    self.map = None;
                    said(e)
                })?;
                match step {
                    Step::Get(next) => json!({ "request": keyed(&next, arg("key")?) }),
                    Step::Choose(found) => {
                        self.map = None;
                        json!({ "choose": found })
                    }
                    Step::Done(map) => {
                        self.map = None;
                        json!({ "map": {
                            "fetched": map.fetched(arg("at")?),
                            "pin": map.pin,
                            "place": map.place_name(),
                            "png": base64::engine::general_purpose::STANDARD.encode(&map.png),
                        } })
                    }
                }
            }
            "make" => {
                let shipped = shipped(arg("theme")?)?;
                self.theme = arg("theme")?.to_string();
                let w = self.generation.as_mut().ok_or_else(|| said("no generation has begun: `begin` first"))?;
                // Each icon the slides name, found in the set that ships (PLAN G.23).
                let words = w.generation.icon_words();
                w.generation.give_icons(scaena_ops::narrative::icons_found(&words));
                let narrative =
                    w.generation.narrative().ok_or_else(|| said("make: the narrative is not written yet"))?;
                let model: scaena_core::model::theme::Theme =
                    serde_json::from_str(shipped.text).map_err(|e| said(format!("the theme: {e}")))?;
                let (placed, assets, unmade) = self.painted.clone().unwrap_or_default();
                let mut built = scaena_ops::narrative::deck(narrative, &model, &placed).map_err(said)?;
                built.assets = assets;
                let mut fonts: BTreeMap<String, Vec<u8>> =
                    shipped.fonts().map(|(p, b)| (p.to_string(), b.to_vec())).collect();
                let theme = match &self.script {
                    Some((script, bytes)) => {
                        fonts.insert(script.file.to_string(), bytes.clone());
                        scaena_ops::scripts::taken(shipped.text, script).map_err(said)?
                    }
                    None => shipped.text.to_string(),
                };
                let fallback = w.generation.fallback().map(str::to_string);
                let arc = narrative.arc.clone();
                let mut making = Making::new(built, &arc, fallback, shipped.file, &theme, &fonts).map_err(said)?;
                if !placed.is_empty() || !unmade.is_empty() {
                    making.painted(placed.len(), unmade);
                }
                // With a key, the model is shown each slide as it is drawn (PLAN G.13), unless
                // `look` says not; `looked` takes its answer, and then asks for the fit.
                let key = args.get("key").and_then(Value::as_str);
                let looking = key.filter(|_| args.get("look").and_then(Value::as_bool) != Some(false));
                if let Some(key) = looking {
                    let seen = scaena_ops::narrative::seeing(narrative, &model, &placed, making.layouts());
                    // A slide that would not draw for the model to see leaves the deck as made,
                    // saying why (narrative STAB-3): the look is a polish, never the deck.
                    match making.drawn(scaena_ops::narrative::LOOK_WIDTH) {
                        Ok(drawn) => {
                            let mut look = Look { seen, drawn, ..Look::default() };
                            if let Some(rounds) = look.rounds(&w.generation, key) {
                                self.look = look;
                                self.making = Some((making, Vec::new()));
                                return Ok(json!({ "look": rounds }));
                            }
                        }
                        Err(why) => making.unlooked(format!("the slides could not be drawn for the model: {why}")),
                    }
                }
                let fit = self.fitting(making, key)?;
                json!({ "fit": fit })
            }
            "looked" => {
                // The model's answers to the look's rounds (narrative PERF-5), every one, in the
                // order `make` gave the rounds: `replies`, each `{status, statusText, body}` (lift-up
                // 20261025 PERF-2). Each slide's change is kept where lint finds the slide no worse,
                // then the fit asked for, as `make` asks it: `{looked, unlooked, fit}`. A round not
                // answered loses its own slides' changes alone, said beside the rest.
                let w = self.generation.as_ref().ok_or_else(|| said("looked: no narrative is written"))?;
                let key = args.get("key").and_then(Value::as_str);
                let replies = (args.get("replies").and_then(Value::as_array))
                    .ok_or_else(|| said("looked: `replies`, the answer to each round, is an array it needs"))?;
                if replies.len() != self.look.asked.len() {
                    return Err(said(format!(
                        "looked: {} replies to {} rounds: answer each round, in order",
                        replies.len(),
                        self.look.asked.len()
                    )));
                }
                let mut changes = Vec::new();
                let mut unanswered: Option<String> = None;
                for (round, reply) in self.look.asked.iter().zip(replies) {
                    let shown = &self.look.seen[round.clone()];
                    let status =
                        reply.get("status").and_then(Value::as_u64).and_then(|s| u16::try_from(s).ok()).unwrap_or(0);
                    let status_text = reply.get("statusText").and_then(Value::as_str).unwrap_or_default();
                    let body = reply.get("body").and_then(Value::as_str).unwrap_or_default();
                    match w.generation.look_answer(status, status_text, body, shown) {
                        Ok(made) => changes.extend(made),
                        Err(why) => {
                            let (first, last) = (shown[0].slide, shown[shown.len() - 1].slide);
                            let why = match shown.len() {
                                1 => format!("slide {first} not looked at: {why}"),
                                _ => format!("slides {first}\u{2013}{last} not looked at: {why}"),
                            };
                            unanswered = Some(unanswered.map_or(why.clone(), |was| format!("{was}; {why}")));
                        }
                    }
                }
                let (mut making, _) = self.making.take().ok_or_else(|| said("looked: no deck is made"))?;
                let narrative = w.generation.narrative().ok_or_else(|| said("looked: no narrative is written"))?;
                self.look = Look::default();
                if !changes.is_empty() {
                    let shipped = shipped(&self.theme)?;
                    let model: scaena_core::model::theme::Theme =
                        serde_json::from_str(shipped.text).map_err(|e| said(format!("the theme: {e}")))?;
                    let (placed, assets, _) = self.painted.clone().unwrap_or_default();
                    let shown =
                        scaena_ops::narrative::Shown { narrative, theme: &model, pictures: &placed, assets: &assets };
                    making.look(shown, changes).map_err(said)?;
                }
                if let Some(why) = unanswered {
                    let was = making.report()["unlooked"].as_str().map(str::to_string);
                    making.unlooked(was.map_or(why.clone(), |was| format!("{was}; {why}")));
                }
                let report = making.report();
                let fit = self.fitting(making, key)?;
                json!({ "looked": report["looked"].as_array().map_or(0, Vec::len), "unlooked": report["unlooked"], "fit": fit })
            }
            "fitted" => {
                let key = args.get("key").and_then(Value::as_str);
                let w = self.generation.as_ref().ok_or_else(|| said("fitted: no narrative is written"))?;
                let (mut making, longs) = self.making.take().ok_or_else(|| said("fitted: no deck is made"))?;
                match w.generation.fit_answer(status(), &status_text(), arg("body")?, &longs) {
                    Ok(rewrites) => {
                        let n = rewrites.len();
                        making.fitted(rewrites).map_err(said)?;
                        // Asked once more for what the rewrite left too long, then said (STAB-4).
                        self.fit_rounds += 1;
                        let fit = match self.fit_rounds < scaena_narrato::fit::ROUNDS {
                            true => self.fitting(making, key)?,
                            false => {
                                self.making = Some((making, Vec::new()));
                                Value::Null
                            }
                        };
                        if fit.is_null()
                            && let Some((making, _)) = self.making.as_mut()
                        {
                            making.still_long().map_err(said)?;
                        }
                        json!({ "fitted": n, "fit": fit })
                    }
                    Err(why) => {
                        making.unfitted(why.to_string());
                        self.making = Some((making, longs));
                        json!({ "unfitted": why.to_string() })
                    }
                }
            }
            "report" => {
                let (making, _) = self.making.as_ref().ok_or_else(|| said("report: no deck is made"))?;
                let mut out = making.report();
                out["sources"] = json!(self.generation.as_ref().map(|w| &w.given));
                out
            }
            _ => return Err(said(format!("`{method}` is no step of a generation"))),
        })
    }

    /// The bundle made, each sheet given kept as a data source, opened as a session: none where
    /// the deck would not validate, the report then saying why.
    pub fn open(&mut self) -> Result<Session, Failure> {
        let (making, _) = self.making.as_mut().ok_or_else(|| said("open: no deck is made"))?;
        making.keep(&scaena_sources::tables(&self.sources)).map_err(said)?;
        let files = making.files().map_err(said)?.ok_or_else(|| {
            let report = making.report();
            let errors: Vec<String> = (report["created"]["findings"].as_array().into_iter().flatten())
                .filter(|f| f["severity"] == "error")
                .map(|f| format!("{} {}", f["code"].as_str().unwrap_or(""), f["message"].as_str().unwrap_or("")))
                .collect();
            said(format!("the deck made would not validate: {}", errors.join("; ")))
        })?;
        Session::open(files).map_err(said)
    }

    /// `making` kept, and with `key`, the request that asks for its texts that still run past
    /// their place shorter: `{request, texts}`, or null.
    fn fitting(&mut self, making: Making, key: Option<&str>) -> Result<Value, Failure> {
        let longs = making.longs().map_err(said)?;
        let fit = match (key, &self.generation) {
            (Some(key), Some(w)) if !longs.is_empty() => w.generation.fit_request(key, &longs).map(request),
            _ => None,
        };
        let texts = longs.len();
        self.making = Some((making, longs));
        Ok(fit.map_or(Value::Null, |r| json!({ "request": r, "texts": texts })))
    }

    fn written(&mut self) -> Result<&mut Written, Failure> {
        self.generation.as_mut().ok_or_else(|| said("no generation has begun: `begin` first"))
    }

    fn keep(&mut self, source: Source) -> Result<Value, Failure> {
        if source.pdf.is_none() && source.text.trim().is_empty() {
            return Err(said(format!("{}: holds no words to ground a deck in", source.name)));
        }
        let said = read(&source);
        self.sources.push(source);
        Ok(said)
    }
}

/// A font a theme that ships names, by its path.
fn shipped_font(shipped: &scaena_ops::shipped::Shipped, file: &str) -> Result<Vec<u8>, scaena_ops::OpsError> {
    let bytes = scaena_ops::shipped::font(file).map(<[u8]>::to_vec);
    bytes.ok_or_else(|| scaena_ops::OpsError::new(format!("{} names `{file}`, which does not ship", shipped.name)))
}

/// The theme that ships as `name`.
fn shipped(name: &str) -> Result<scaena_ops::shipped::Shipped, Failure> {
    scaena_ops::shipped::theme(name)
        .ok_or_else(|| said(format!("`{name}` is not a theme that ships: {}", scaena_ops::shipped::names())))
}

/// Deep research's report as a source of its own, named as Vzestup names it.
fn researched(topic: &str, report: &Report) -> Source {
    Source { name: source_name(topic), kind: Kind::Research, text: report.source_text(), sheets: Vec::new(), pdf: None }
}

/// Where a generation stands: `{step, slides, arc?, reasoning?, fallback?, written?, truncated?,
/// outline?, plan?, wait?}`, the outline once written or settled (narrative FUNC-1), the beats the
/// slides are spread over once the arc is chosen, and `wait` the seconds to wait before the same
/// step is asked again, where its last answer was for now (narrative REL-1).
fn where_it_stands(g: &Generation) -> Value {
    let narrative = g.narrative();
    let step: Step = g.step();
    json!({
        "step": step,
        "slides": g.target(),
        "arc": g.arc().map(|a| a.selection.clone()),
        "reasoning": g.arc().map(|a| a.reasoning.clone()),
        "fallback": g.fallback(),
        "written": narrative.map(|n| n.slides.len()),
        "truncated": narrative.map(|n| n.truncated),
        "outline": g.outline(),
        "plan": g.plan(),
        "wait": g.again(),
        "tried": g.tried(),
    })
}
