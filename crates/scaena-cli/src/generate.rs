//! `scaena generate` (PLAN G.2–G.4, ADR-0026): a deck made by Vzestup's Narrato model from a
//! prompt, with the user's own key, read from the environment, and grounded in the sources
//! given (`crate::sources`). `scaena-narrato` builds each request and reads each answer; the
//! calls are made here, the CLI being the client.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use scaena_narrato::brief::MODALITIES;
use scaena_narrato::pictures::{self, Look, Quality};
use scaena_narrato::{Brief, Generation, Provider, Request, Settings};
use scaena_ops::create::ThemeSource;
use scaena_ops::narrative::{Looking, Painter, Progress, Reply, Transport};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Args)]
pub struct GenerateArgs {
    /// The bundle to make: a directory not there yet, or empty.
    dir: Option<PathBuf>,
    /// What the deck is about: the brief's topic, in the user's words.
    prompt: Option<String>,
    /// A theme that ships (`dusk`, `daybreak`, `ember`, `gallery`), which comes with its fonts; or a theme
    /// file, whose fonts are beside it or above it.
    #[arg(long, default_value = "dusk")]
    theme: String,
    /// `anthropic`, `openai`, or `gemini`; else the first whose key the environment holds:
    /// ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY (or GOOGLE_API_KEY).
    #[arg(long)]
    provider: Option<String>,
    /// The model, one of those `--models` lists; else $SCAENA_MODEL.
    #[arg(long)]
    model: Option<String>,
    /// The API's address, where it is not the provider's own.
    #[arg(long)]
    base: Option<String>,
    /// List the models the key can use, and make nothing.
    #[arg(long)]
    models: bool,
    /// Who the deck is for ("Investors", "a school board").
    #[arg(long)]
    audience: Option<String>,
    /// Inform, Persuade, Inspire, Sell, Educate, or Update Status.
    #[arg(long)]
    goal: Option<String>,
    /// "5-10 slides", "11-15 slides", "16-25 slides", "26-40 slides", or "41+ slides".
    #[arg(long)]
    length: Option<String>,
    /// How it is delivered: in-person, virtual-live, hybrid-live, keynote, webinar,
    /// recorded-video, or asynchronous.
    #[arg(long)]
    modality: Option<String>,
    /// What it should achieve, in your words.
    #[arg(long)]
    outcome: Option<String>,
    /// The language its words are in, by its English name.
    #[arg(long)]
    language: Option<String>,
    /// One of Narrato's personas, by its id, for an audience it knows.
    #[arg(long)]
    persona: Option<String>,
    /// The voice its words take: tone, vocabulary, posture.
    #[arg(long)]
    voice: Option<String>,
    /// The arc it follows, by its name, instead of the one the model would choose.
    #[arg(long)]
    arc: Option<String>,
    /// Make the deck from slides written elsewhere, the JSON Narrato's slides are written in,
    /// with no call to a model.
    #[arg(long)]
    slides: Option<PathBuf>,
    /// What the deck is grounded in, given once for each: a file (Markdown, text, RTF, CSV,
    /// Excel, Word, PowerPoint, HTML, JSON, or PDF); `sec:TICKER`, or `sec:TICKER:10-K,8-K`, a
    /// company's latest filings and yearly figures on SEC's EDGAR; `github:OWNER/REPO`, a
    /// repository's README, documentation, and manifests ($GITHUB_TOKEN for a private one); a
    /// Google document, spreadsheet, or presentation shared by link; or a web address.
    #[arg(long)]
    source: Vec<String>,
    /// Your email, which SEC asks every request to carry; else $SCAENA_SEC_CONTACT.
    #[arg(long)]
    sec_contact: Option<String>,
    /// Ground the deck in deep research on its topic first, by your key: Gemini's research
    /// agent, or Anthropic's model with web search. It takes minutes; its report is a source.
    #[arg(long)]
    research: bool,
    /// Who makes the slides' pictures (PLAN G.10): `openai` or `gemini`, with its key from the
    /// environment, or `none`; else the first of those whose key the environment holds. Two in
    /// five slides get one, ten at most: the cover, the dividers, the ending, and the rest by
    /// how much each wants one.
    #[arg(long)]
    pictures: Option<String>,
    /// The image model, one of those `--picture-models` lists; else the best the key can use.
    #[arg(long)]
    picture_model: Option<String>,
    /// What the pictures are made as: `illustration` (in the theme's colors, the default) or
    /// `photo`.
    #[arg(long, default_value = "illustration")]
    picture_look: String,
    /// How much care each picture is made with, and what it costs: `low`, `medium`, or `high`.
    #[arg(long, default_value = "medium")]
    picture_quality: String,
    /// List the image models the picture provider's key can use, best first, and make nothing.
    #[arg(long)]
    picture_models: bool,
    /// The image API's address, where it is not the provider's own.
    #[arg(long)]
    picture_base: Option<String>,
    /// Make the deck without showing the model each slide as it is drawn (PLAN G.13). Shown
    /// them, it may give a slide another of its layouts and tighten its words, each kept where
    /// lint finds the slide no worse for it.
    #[arg(long)]
    no_look: bool,
    /// What a model said it would change on the deck it saw drawn, written elsewhere, the JSON
    /// the look is answered in: taken with no call, as `--slides` gives the slides.
    #[arg(long)]
    look_answer: Option<PathBuf>,
}

/// The environment variables a provider's key is read from, in order.
fn key_vars(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Anthropic => &["ANTHROPIC_API_KEY"],
        Provider::Openai => &["OPENAI_API_KEY"],
        Provider::Gemini => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    }
}

/// The key the environment holds for `provider`.
fn key(provider: Provider) -> Option<String> {
    key_vars(provider).iter().find_map(|v| std::env::var(v).ok().filter(|k| !k.trim().is_empty()))
}

fn no_key(provider: Provider) -> anyhow::Error {
    anyhow!("no {} key: set {}, with a key of your own", provider.name(), key_vars(provider).join(" or "))
}

/// The client every call goes through: an error status read as an answer, the environment's
/// proxy, the system's certificates, and room for an answer that takes minutes to write.
pub(crate) fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(20 * 60)))
        .tls_config(tls)
        .build()
        .new_agent()
}

/// The font a deck in `script` takes (PLAN G.22, ADR-0027): kept by its digest in this user's
/// cache (`$XDG_CACHE_HOME/scaena/fonts`, else `~/.cache/scaena/fonts`), else fetched from
/// google/fonts (or `$SCAENA_FONTS_BASE`, where a test serves it), checked against the digest
/// `scaena_ops::scripts` pins, and kept.
fn script_font(agent: &ureq::Agent, script: &scaena_ops::scripts::Script, json: bool) -> Result<Vec<u8>> {
    let cache = (std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".cache")))
        .map(|cache| cache.join("scaena/fonts").join(script.sha256));
    if let Some(bytes) = cache.as_ref().and_then(|kept| std::fs::read(kept).ok())
        && script.check(&bytes).is_ok()
    {
        return Ok(bytes);
    }
    let url = script.url(env(&["SCAENA_FONTS_BASE"]).as_deref());
    if !json {
        eprintln!(
            "fetching {}, the font a deck in {} is set in ({:.1} MB)…",
            script.family,
            script.language,
            script.bytes as f64 / 1e6
        );
    }
    let unfetched = |why: String| {
        anyhow!(
            "a deck in {} is set in {}, which could not be fetched from {url}: {why}",
            script.language,
            script.family
        )
    };
    let mut response = agent.get(&url).call().map_err(|e| unfetched(e.to_string()))?;
    if !response.status().is_success() {
        return Err(unfetched(response.status().to_string()));
    }
    let bytes =
        (response.body_mut().with_config().limit(32 << 20).read_to_vec()).map_err(|e| unfetched(e.to_string()))?;
    script.check(&bytes)?;
    if let Some(kept) = &cache
        && let Some(dir) = kept.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        // A cache that cannot be written only means the next deck fetches it again.
        let _ = std::fs::write(kept, &bytes);
    }
    Ok(bytes)
}

/// `request` made: its status, its reason, and its body; status 0 where it never came back.
fn fetch(agent: &ureq::Agent, request: &Request) -> Reply {
    let sent = if request.method == "GET" {
        let mut r = agent.get(&request.url);
        for (k, v) in &request.headers {
            r = r.header(k, v);
        }
        r.call()
    } else {
        let mut r = agent.post(&request.url);
        for (k, v) in &request.headers {
            r = r.header(k, v);
        }
        r.send(request.body.as_ref().map(|b| b.to_string()).unwrap_or_default())
    };
    match sent {
        Ok(mut response) => {
            let status = response.status();
            let after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_string);
            let body = response.body_mut().with_config().limit(64 << 20).read_to_string();
            match body {
                Ok(body) => Reply {
                    status: status.as_u16(),
                    status_text: status.canonical_reason().unwrap_or_default().into(),
                    body,
                    after,
                },
                Err(e) => Reply { status: 0, status_text: format!("reading the answer: {e}"), ..Reply::default() },
            }
        }
        Err(e) => Reply { status: 0, status_text: e.to_string(), ..Reply::default() },
    }
}

/// The CLI's transport: each request made by `fetch`, several at once each on a thread of its
/// own, and a wait slept.
struct Client<'a> {
    agent: &'a ureq::Agent,
}

impl Transport for Client<'_> {
    fn send(&mut self, request: &Request) -> Reply {
        fetch(self.agent, request)
    }

    fn send_all(&mut self, requests: &[Request]) -> Vec<Reply> {
        let agent = self.agent;
        std::thread::scope(|scope| {
            let made: Vec<_> = requests.iter().map(|r| scope.spawn(move || fetch(agent, r))).collect();
            made.into_iter()
                .map(|t| {
                    t.join().unwrap_or_else(|_| Reply {
                        status: 0,
                        status_text: "the call failed".into(),
                        ..Reply::default()
                    })
                })
                .collect()
        })
    }

    fn pause(&mut self, seconds: f64) {
        std::thread::sleep(Duration::from_secs_f64(seconds.clamp(0.0, 120.0)));
    }
}

/// Who makes the pictures, as the arguments and the environment say: a provider whose key it
/// holds, and the model named or the best its key can use; none where `--pictures none`, or no
/// key for one is held.
fn painter(a: &GenerateArgs, agent: &ureq::Agent) -> Result<Option<(pictures::Settings, String, Look)>> {
    let provider = match a.pictures.as_deref().map(str::trim) {
        Some(none) if none.eq_ignore_ascii_case("none") => return Ok(None),
        Some(name) => {
            let p = Provider::named(name).filter(|p| *p != Provider::Anthropic);
            let p = p.ok_or_else(|| anyhow!("`{name}` makes no pictures: openai, gemini, or none"))?;
            if key(p).is_none() {
                return Err(no_key(p));
            }
            p
        }
        None => match [Provider::Openai, Provider::Gemini].into_iter().find(|p| key(*p).is_some()) {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    let key = key(provider).ok_or_else(|| no_key(provider))?;
    let look = Look::named(&a.picture_look)
        .ok_or_else(|| anyhow!("`{}` is no look: illustration or photo", a.picture_look))?;
    let quality = Quality::named(&a.picture_quality)
        .ok_or_else(|| anyhow!("`{}` is no quality: low, medium, or high", a.picture_quality))?;
    let model = match &a.picture_model {
        Some(m) => m.clone(),
        None => {
            let reply = fetch(agent, &provider.models(&key, a.picture_base.as_deref(), false));
            let listed =
                pictures::models(provider, reply.status, &reply.status_text, &reply.body).map_err(|e| anyhow!(e))?;
            listed.into_iter().next().ok_or_else(|| {
                anyhow!("your {} key lists no image model: --pictures none makes the deck without", provider.name())
            })?
        }
    };
    Ok(Some((pictures::Settings { provider, model, base: a.picture_base.clone(), quality }, key, look)))
}

/// How far a generation has come, in words.
fn said(p: &Progress) -> String {
    match p {
        Progress::Choosing => "choosing the arc…".into(),
        Progress::Chose { arc, fallback: None } => format!("arc: {arc}"),
        Progress::Chose { arc, fallback: Some(why) } => format!("arc: {arc}, the heuristic's ({why})"),
        Progress::Writing { slides } => format!("writing the slides, at least {slides}…"),
        Progress::Wrote { slides, truncated: false } => format!("{slides} slides written"),
        Progress::Wrote { slides, truncated: true } => format!("{slides} slides written, the answer cut short"),
        Progress::Fitting { texts } => format!("{texts} texts run past their place: asking for them shorter…"),
        Progress::Fitted { texts } => format!("{texts} rewritten to fit"),
        Progress::Painting { pictures } => format!("making {pictures} pictures for the slides… (a minute or two)"),
        Progress::Looking { slides } => format!("showing the model the {slides} slides as drawn…"),
        Progress::Looked { slides } => format!("{slides} slides changed by what the model saw"),
        Progress::Painted { pictures, unmade } if unmade.is_empty() => format!("{pictures} pictures made"),
        Progress::Painted { pictures, unmade } => {
            format!("{pictures} pictures made; not made:\n  {}", unmade.join("\n  "))
        }
    }
}

/// The deep research the brief asks for (PLAN G.8), as a source: the report and its sources,
/// polled or resumed as `scaena_narrato::research` asks, each wait kept here.
fn research(
    brief: &Brief,
    provider: Provider,
    model: &str,
    base: Option<&str>,
    key: &str,
    agent: &ureq::Agent,
    json: bool,
) -> Result<scaena_sources::Source> {
    use scaena_narrato::research::{Asked, Next, Research, source_name};
    // Researched for the brief as Narrato reads it, its defaults filled.
    let filled = brief.clone().validated().map_err(|e| anyhow!(e))?;
    let asked = Asked { topic: filled.topic, audience: filled.audience, goal: filled.goal, length: filled.length };
    let mut research = Research::new(&asked, provider, model, base, false).map_err(|e| anyhow!(e))?;
    if !json {
        eprintln!("researching {}… (minutes)", asked.topic);
    }
    let mut request = research.start(key);
    let mut said: Option<String> = None;
    let report = loop {
        let reply = fetch(agent, &request);
        match research
            .answer(key, reply.status, &reply.status_text, &reply.body)
            .map_err(|e| anyhow!("deep research: {e}"))?
        {
            Next::Send(next) => request = next,
            Next::Wait { ms, then } => {
                if !json && research.progress != said {
                    said.clone_from(&research.progress);
                    if let Some(progress) = &said {
                        eprintln!("  {progress}");
                    }
                }
                std::thread::sleep(Duration::from_millis(ms));
                request = then;
            }
            Next::Done(report) => break report,
        }
    };
    if !json {
        let cut = if report.cut { ", cut short" } else { "" };
        eprintln!("researched: {} sources{cut}", report.sources.len());
    }
    Ok(scaena_sources::Source {
        name: source_name(&asked.topic),
        kind: scaena_sources::Kind::Research,
        text: report.source_text(),
        sheets: Vec::new(),
        pdf: None,
    })
}

/// The first of `vars` the environment holds, not blank.
fn env(vars: &[&str]) -> Option<String> {
    vars.iter().find_map(|v| std::env::var(v).ok().filter(|k| !k.trim().is_empty()))
}

/// `scaena generate`: the models a key can use, or a deck made from a prompt, or from slides
/// written elsewhere, into a new bundle. Exit 1 when the deck has errors, 2 when what was given
/// is not what it takes or a call failed.
pub fn run(a: GenerateArgs, json: bool) -> Result<ExitCode> {
    let provider = match &a.provider {
        Some(name) => {
            Provider::named(name).ok_or_else(|| anyhow!("`{name}` is no provider: anthropic, openai, or gemini"))?
        }
        None => Provider::ALL.into_iter().find(|p| key(*p).is_some()).unwrap_or(Provider::Anthropic),
    };
    let model = a.model.clone().or_else(|| std::env::var("SCAENA_MODEL").ok()).unwrap_or_default();
    if a.picture_models {
        let provider = match a.pictures.as_deref() {
            Some(name) => Provider::named(name).ok_or_else(|| anyhow!("`{name}` is no provider: openai or gemini"))?,
            None => {
                [Provider::Openai, Provider::Gemini].into_iter().find(|p| key(*p).is_some()).unwrap_or(Provider::Openai)
            }
        };
        let key = key(provider).ok_or_else(|| no_key(provider))?;
        let reply = fetch(&agent(), &provider.models(&key, a.picture_base.as_deref(), false));
        let models =
            pictures::models(provider, reply.status, &reply.status_text, &reply.body).map_err(|e| anyhow!(e))?;
        if json {
            println!("{}", serde_json::json!({ "provider": provider.name(), "models": models }));
        } else {
            models.iter().for_each(|m| println!("{m}"));
        }
        return Ok(ExitCode::SUCCESS);
    }
    if a.models {
        let key = key(provider).ok_or_else(|| no_key(provider))?;
        let reply = fetch(&agent(), &provider.models(&key, a.base.as_deref(), false));
        let models = provider.read_models(reply.status, &reply.status_text, &reply.body).map_err(|e| anyhow!(e))?;
        if json {
            println!("{}", serde_json::json!({ "provider": provider.name(), "models": models }));
        } else {
            models.iter().for_each(|m| println!("{m}"));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let dir = a.dir.as_deref().context("say where the bundle goes: scaena generate DIR \"what it is about\"")?;
    // Before a call is paid for: the place, the theme, the brief, the key, then the sources.
    scaena_ops::create::vacant(dir)?;
    let theme = ThemeSource::of(Path::new(&a.theme))?;
    let agent = agent();
    // A deck in a script the theme's fonts do not draw takes the script's font, fetched once and
    // kept (PLAN G.22): by the brief's language, or by what slides written elsewhere are in.
    let written = match &a.slides {
        Some(path) => Some(std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?),
        None => None,
    };
    let needed = scaena_ops::scripts::needed(
        a.language.as_deref(),
        written.as_deref().unwrap_or_default(),
        &theme.text,
        &|file| theme.font(file),
    )?;
    let script = match needed {
        Some(script) => Some((script, script_font(&agent, script, json)?)),
        None => None,
    };
    let theme_text = match script {
        Some((script, _)) => scaena_ops::scripts::taken(&theme.text, script)?,
        None => theme.text.clone(),
    };
    let font = |file: &str| match &script {
        Some((script, bytes)) if script.file == file => Ok(bytes.clone()),
        _ => theme.font(file),
    };
    // A deck in letters the theme's fonts cannot draw would be blank where its words stand.
    scaena_ops::narrative::writable(a.language.as_deref(), &theme_text, theme.shipped, &font)?;
    if let Some(name) = &a.arc
        && !scaena_narrato::arcs::arcs().iter().any(|arc| arc.arc.eq_ignore_ascii_case(name))
    {
        let names: Vec<&str> = scaena_narrato::arcs::arcs().iter().map(|arc| arc.arc.as_str()).collect();
        bail!("`{name}` is no arc of Narrato's: {}", names.join(", "));
    }
    let modality = match &a.modality {
        Some(m) => Some(
            MODALITIES
                .iter()
                .find(|(id, label)| id.eq_ignore_ascii_case(m) || label.eq_ignore_ascii_case(m))
                .map(|(_, label)| label.to_string())
                .with_context(|| {
                    let ids: Vec<&str> = MODALITIES.iter().map(|(id, _)| *id).collect();
                    format!("`{m}` is no way a deck is delivered: {}", ids.join(", "))
                })?,
        ),
        None => None,
    };
    let topic = match (&a.prompt, &a.slides) {
        (Some(prompt), _) => prompt.clone(),
        (None, Some(path)) => path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        (None, None) => bail!("say what the deck is about: scaena generate DIR \"what it is about\""),
    };
    let mut brief = Brief {
        topic,
        audience: a.audience.clone().unwrap_or_default(),
        goal: a.goal.clone().unwrap_or_default(),
        length: a.length.clone().unwrap_or_default(),
        modality,
        desired_outcomes: a.outcome.clone(),
        language: a.language.clone(),
        persona_id: a.persona.clone(),
        voice: a.voice.clone(),
        ..Brief::default()
    };
    let key = match &a.slides {
        Some(_) => None,
        None => {
            let key = key(provider).ok_or_else(|| no_key(provider))?;
            if model.is_empty() {
                bail!(
                    "name the model: --model, or $SCAENA_MODEL; `scaena generate --models` lists those your {} key can use",
                    provider.name()
                );
            }
            Some(key)
        }
    };
    let reach = scaena_sources::fetch::Reach {
        contact: a.sec_contact.clone().or_else(|| env(&["SCAENA_SEC_CONTACT"])),
        token: env(&["GITHUB_TOKEN", "GH_TOKEN"]),
        browser: false,
    };
    let mut read = Vec::new();
    for spec in &a.source {
        if !json {
            eprintln!("reading {spec}…");
        }
        read.push(crate::sources::read(spec, &agent, &reach).with_context(|| format!("--source {spec}"))?);
    }
    if a.research {
        let key = key.as_deref().context("deep research asks a model: --slides makes no call")?;
        read.push(research(&brief, provider, &model, a.base.as_deref(), key, &agent, json)?);
    }
    let grounding = scaena_ops::narrative::grounding(&read, provider)?;
    brief.source_content = grounding.content.clone();
    let settings = Settings { provider, model: model.clone(), base: a.base.clone(), browser: false };
    let mut generation = Generation::new(brief, settings).map_err(|e| anyhow!(e))?;
    match &a.slides {
        Some(path) => {
            generation.choose_arc(a.arc.as_deref());
            let text = written.as_deref().unwrap_or_default();
            generation.written(text, false).map_err(|e| anyhow!("{}: {e}", path.display()))?;
        }
        None if a.arc.is_some() => generation.choose_arc(a.arc.as_deref()),
        None => {}
    }
    let painting = painter(&a, &agent)?;
    let painter =
        painting.as_ref().map(|(settings, key, look)| Painter { settings: settings.clone(), key, look: *look });
    if !json && let Some(p) = &painter {
        eprintln!(
            "pictures by {}'s {}, made as {}s",
            p.settings.provider.name(),
            p.settings.model,
            match p.look {
                Look::Illustration => "illustration",
                Look::Photo => "photo",
            }
        );
    }
    let mut client = Client { agent: &agent };
    let mut say = |p: Progress| {
        if !json {
            eprintln!("{}", said(&p));
        }
    };
    let answered = match &a.look_answer {
        Some(path) => Some(std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?),
        None => None,
    };
    let looking = match (&answered, a.no_look) {
        (Some(text), _) => Looking::Answered(text),
        (None, true) => Looking::No,
        (None, false) => Looking::Asked,
    };
    let (made, write) = scaena_ops::narrative::generate(
        generation,
        key.as_deref(),
        &grounding.documents,
        painter.as_ref(),
        looking,
        &theme.file,
        theme_text.clone(),
        &font,
        &mut client,
        &mut say,
    )?;
    let mut kept = Vec::new();
    if let Some(write) = write {
        // Each sheet given, a data source of the deck's, which a chart can plot.
        let (write, sheets) = scaena_ops::narrative::keeping(write, &scaena_sources::tables(&read))?;
        kept = sheets;
        scaena_ops::create::make(dir, write)?;
    }
    if json {
        let mut out = serde_json::to_value(&made)?;
        out["sources"] = serde_json::to_value(&grounding.given)?;
        out["data"] = serde_json::to_value(&kept)?;
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else if made.created.created {
        let d = dir.display();
        println!("made {d}: {} slides on the arc {}", made.layouts.len(), made.arc);
        println!("  layouts: {}", made.layouts.join(", "));
        if !made.fixed.is_empty() {
            println!("  fixed by lint: {}", made.fixed.len());
        }
        if !made.fitted.is_empty() {
            println!("  rewritten to fit: {}", made.fitted.join(", "));
        }
        if let Some(why) = &made.unfitted {
            println!("  not rewritten to fit: {why}");
        }
        for c in &made.looked {
            let layout = c.layout.as_deref().map(|l| format!(" as {l}")).unwrap_or_default();
            let words =
                if c.title.is_some() || c.subtitle.is_some() || c.bullets.is_some() { ", words tightened" } else { "" };
            println!("  slide {}{layout}{words}: {}", c.slide, c.why);
        }
        match (&made.unlooked, made.looked.is_empty()) {
            (Some(why), true) => println!("  not changed by what the model saw: {why}"),
            (Some(why), false) => println!("  {why}"),
            (None, _) => {}
        }
        if made.pictures > 0 {
            println!("  pictures: {}", made.pictures);
        }
        for g in &grounding.given {
            let cut = if g.truncated { ", cut short to fit" } else { "" };
            println!("  grounded in {} ({}, {} KB{cut})", g.name, g.kind, g.bytes.div_ceil(1024));
        }
        for k in &kept {
            match (&k.id, &k.why) {
                (Some(id), _) => println!("  data: @{id} from {}", k.sheet),
                (None, Some(why)) => println!("  data: {} not kept: {why}", k.sheet),
                (None, None) => {}
            }
        }
        crate::report(&made.created.findings, false);
        println!("next: scaena serve {d}");
    } else {
        println!("not made: {} would not validate", dir.display());
        crate::report(&made.created.findings, false);
    }
    Ok(if !made.created.created || made.created.errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
