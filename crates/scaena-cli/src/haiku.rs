//! `scaena haiku` (PLAN O.4e, ADR-0030 decisions 26–29): a haiku about a topic, a slide, or the
//! whole talk, written by the model your key reaches, counted by the engine, asked once more where
//! it is off 5-7-5, and placed in one patch. `scaena_chat::once` and `answered` build and read
//! each request, which `ureq` makes; `scaena_ops::haiku` says what to ask and writes the deck.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use scaena_chat::Provider;
use scaena_core::writing::haiku as words;
use scaena_ops::haiku::{Answered, Poem, Where, Writer};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Args)]
pub struct HaikuArgs {
    /// The bundle: a directory or a `.scaena` file.
    bundle: PathBuf,
    /// What the haiku is about. Without it, the slide `--slide` names, else the whole talk.
    topic: Option<String>,
    /// The state whose slide the haiku is about.
    #[arg(long)]
    slide: Option<String>,
    /// Put it on a new Haiku slide after this state's slide (the default: after `--slide`, else
    /// after the last).
    #[arg(long, conflicts_with_all = ["on", "replace"])]
    after: Option<String>,
    /// Put it in this state's slide's empty verse place.
    #[arg(long, conflicts_with = "replace")]
    on: Option<String>,
    /// Put it over this verse text, the looks of the words it keeps kept.
    #[arg(long)]
    replace: Option<String>,
    /// Who wrote it, or what it is on, under it; never filled in for you.
    #[arg(long)]
    attribution: Option<String>,
    /// The language it is written in (BCP 47); else the deck's.
    #[arg(long)]
    lang: Option<String>,
    /// The poem's lines, `\n` between them, or `-` to read them from stdin: placed as they are,
    /// with no model asked.
    #[arg(long)]
    lines: Option<String>,
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
    /// List the models the key can use, and write nothing.
    #[arg(long)]
    models: bool,
    /// Say what it would write, and write nothing.
    #[arg(long)]
    dry_run: bool,
}

fn key_vars(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Anthropic => &["ANTHROPIC_API_KEY"],
        Provider::Openai => &["OPENAI_API_KEY"],
        Provider::Gemini => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    }
}

fn key(provider: Provider) -> Option<String> {
    key_vars(provider).iter().find_map(|v| std::env::var(v).ok().filter(|k| !k.trim().is_empty()))
}

fn named(name: &str) -> Option<Provider> {
    Provider::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(name))
}

/// A request the chat crate built, made.
fn sent(request: &scaena_chat::Request) -> scaena_ops::narrative::Reply {
    let request = scaena_narrato::Request {
        method: request.method.clone(),
        url: request.url.clone(),
        headers: request.headers.clone(),
        body: request.body.clone(),
    };
    crate::generate::fetch(&crate::generate::agent(), &request)
}

pub fn run(a: HaikuArgs, json: bool) -> Result<ExitCode> {
    let b = scaena_ops::open(&a.bundle).map_err(|e| anyhow!(e))?;
    let provider = match &a.provider {
        Some(name) => named(name).ok_or_else(|| anyhow!("`{name}` is no provider: anthropic, openai, or gemini"))?,
        None => Provider::ALL.into_iter().find(|p| key(*p).is_some()).unwrap_or(Provider::Anthropic),
    };
    let no_key =
        || anyhow!("no {} key: set {}, with a key of your own", provider.name(), key_vars(provider).join(" or "));
    if a.models {
        let key = key(provider).ok_or_else(no_key)?;
        let reply = sent(&provider.models(&key, a.base.as_deref(), false));
        let models = provider.read_models(reply.status, &reply.status_text, &reply.body).map_err(|e| anyhow!(e))?;
        if json {
            println!("{}", serde_json::json!({ "provider": provider.name(), "models": models }));
        } else {
            models.iter().for_each(|m| println!("{m}"));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let (about, asking) = scaena_ops::haiku::asking(&b, a.topic.as_deref(), a.slide.as_deref(), a.lang.as_deref(), None)
        .map_err(|e| anyhow!(e))?;
    let lang = asking.lang.clone();
    let (poem, note, by) = match &a.lines {
        Some(lines) => {
            let text = if lines == "-" {
                let mut read = String::new();
                std::io::stdin().read_to_string(&mut read).context("reading the lines from stdin")?;
                read
            } else {
                lines.replace("\\n", "\n")
            };
            let poem = Poem::counted(words::taken(&text, &lang), &lang);
            (poem, None, Writer { model: String::new(), provider: None })
        }
        None => {
            let key = key(provider).ok_or_else(no_key)?;
            let model = a.model.clone().or_else(|| std::env::var("SCAENA_MODEL").ok()).filter(|m| !m.trim().is_empty());
            let model = model.ok_or_else(|| anyhow!("name a model with --model or $SCAENA_MODEL (--models lists them)"))?;
            let ask = |prompt: &str| -> Result<Poem> {
                let request =
                    scaena_chat::once(provider, &model, a.base.as_deref(), &key, false, &asking.instructions, prompt, None);
                let reply = sent(&request);
                let said = scaena_chat::answered(provider, reply.status, &reply.status_text, &reply.body)
                    .map_err(|e| anyhow!("{}: {e}", provider.name()))?;
                let poem = Poem::counted(words::taken(&said, &lang), &lang);
                if poem.lines.is_empty() {
                    bail!("{model} wrote no haiku: ask it again");
                }
                Ok(poem)
            };
            let first = ask(&asking.prompt)?;
            let (poem, note) = match scaena_ops::haiku::answered(&about, &lang, first.clone(), None) {
                Answered::Offer { poem, note } => (poem, note),
                Answered::Again { prompt } => match scaena_ops::haiku::answered(&about, &lang, first, Some(ask(&prompt)?)) {
                    Answered::Offer { poem, note } => (poem, note),
                    Answered::Again { .. } => unreachable!("two answers are offered"),
                },
            };
            (poem, note, Writer { model, provider: Some(provider.name().to_string()) })
        }
    };
    let last = b.deck.states.last().map(|s| s.id.clone()).unwrap_or_default();
    let to = match (&a.after, &a.on, &a.replace) {
        (_, _, Some(node)) => Where::Replace { node: node.clone(), state: a.on.clone() },
        (_, Some(state), None) => Where::On(state.clone()),
        (Some(state), None, None) => Where::After(state.clone()),
        (None, None, None) => Where::After(a.slide.clone().unwrap_or(last)),
    };
    let credit = scaena_ops::haiku::credited(&by, &scaena_ops::haiku::today(), &about);
    let written = scaena_ops::haiku::writing(&b, &poem, note, &to, a.attribution.as_deref(), &credit, a.dry_run)
        .map_err(|e| anyhow!(e))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&written)?);
    } else {
        for (line, count) in written.lines.iter().zip(&poem.checked.lines) {
            println!("{line}    ({})", words::said(count.count));
        }
        if let Some(note) = &written.note {
            println!("{note}");
        }
        let did = if a.dry_run { "Would place it" } else { "Placed it" };
        println!("{did} on {}, as {}.", written.state, written.node);
    }
    Ok(ExitCode::SUCCESS)
}
