//! `scaena generate` (PLAN G.2–G.4, ADR-0026): a deck made by Vzestup's Narrato model from a
//! prompt, with the user's own key, read from the environment, and grounded in the sources
//! given (`crate::sources`). `scaena-narrato` builds each request and reads each answer; the
//! calls are made here, the CLI being the client.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use scaena_narrato::brief::MODALITIES;
use scaena_narrato::{Brief, Generation, Provider, Request, Settings};
use scaena_ops::create::ThemeSource;
use scaena_ops::narrative::{Progress, Reply};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Args)]
pub struct GenerateArgs {
    /// The bundle to make: a directory not there yet, or empty.
    dir: Option<PathBuf>,
    /// What the deck is about: the brief's topic, in the user's words.
    prompt: Option<String>,
    /// A theme that ships (`dusk`, `daybreak`, `ember`), which comes with its fonts; or a theme
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
fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(20 * 60)))
        .tls_config(tls)
        .build()
        .new_agent()
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
            let body = response.body_mut().with_config().limit(64 << 20).read_to_string();
            match body {
                Ok(body) => Reply {
                    status: status.as_u16(),
                    status_text: status.canonical_reason().unwrap_or_default().into(),
                    body,
                },
                Err(e) => Reply { status: 0, status_text: format!("reading the answer: {e}"), body: String::new() },
            }
        }
        Err(e) => Reply { status: 0, status_text: e.to_string(), body: String::new() },
    }
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
    let agent = agent();
    let reach = scaena_sources::fetch::Reach {
        contact: a.sec_contact.clone().or_else(|| env(&["SCAENA_SEC_CONTACT"])),
        token: env(&["GITHUB_TOKEN", "GH_TOKEN"]),
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
            let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            generation.written(&text, false).map_err(|e| anyhow!("{}: {e}", path.display()))?;
        }
        None if a.arc.is_some() => generation.choose_arc(a.arc.as_deref()),
        None => {}
    }
    let mut send = |r: &Request| fetch(&agent, r);
    let mut say = |p: Progress| {
        if !json {
            eprintln!("{}", said(&p));
        }
    };
    let font = |file: &str| theme.font(file);
    let (made, write) = scaena_ops::narrative::generate(
        generation,
        key.as_deref(),
        &grounding.documents,
        &theme.file,
        theme.text.clone(),
        &font,
        &mut send,
        &mut say,
    )?;
    let mut kept = Vec::new();
    if let Some(write) = write {
        // Each sheet given, a data source of the deck's, which a chart can plot.
        let (write, sheets) = scaena_ops::narrative::keeping(write, &read)?;
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
