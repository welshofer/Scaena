//! # scaena-narrato
//!
//! Vzestup's Narrato, ported (ADR-0026, PLAN G.1): a deck's narrative from a brief, by two calls
//! to the user's own model. The first picks one of 13 narrative arcs; the second writes the
//! slides along it, one JSON document under a schema. Both prompts are Narrato's, byte for byte.
//!
//! It makes no HTTP call, reads no clock, and keeps no key: a [`Generation`] gives the client
//! each request to make and reads the answer it hands back, as `scaena-chat` does for the
//! assistant (ADR-0022). A failed arc selection falls back to the heuristic, as Vzestup's does;
//! a failed generation is an error that says why.

pub mod answer;
pub mod arcs;
pub mod beats;
pub mod brief;
pub mod containment;
pub mod duration;
pub mod fit;
pub mod health;
pub mod heuristic;
mod js;
pub mod layouts;
pub mod look;
pub mod pictures;
pub mod prompt;
pub mod providers;
pub mod research;
pub mod schema;

pub use answer::{ChartData, Series, Slide};
pub use brief::{Brief, BriefError};
pub use heuristic::{ArcChoice, Candidate};
pub use providers::{Answer, Document, Picture, Provider, Request, Stop, Usage};

use serde::{Deserialize, Serialize};

/// Room for arc selection's answer, thinking included (Vzestup's `ARC_SELECT_MAX_TOKENS`).
pub const ARC_MAX_TOKENS: u32 = 16_000;
/// Room for the slides, thinking included: a 45-slide deck is some 30 KB of JSON (Vzestup's
/// `SLIDES_MAX_TOKENS`).
pub const SLIDES_MAX_TOKENS: u32 = 64_000;
/// Room for Gemini's answers, either: the most it writes (Vzestup's `maxOutputTokens`).
pub const GEMINI_MAX_TOKENS: u32 = 65_536;
/// Room for the rewrites of texts that run past their place, thinking included.
pub const FIT_MAX_TOKENS: u32 = 16_000;
/// Room for what the model would change on the deck it sees, thinking included.
pub const LOOK_MAX_TOKENS: u32 = 16_000;

/// How many times a step is asked while its provider is busy or does not answer (narrative
/// REL-1): the pictures' six, so that one busy answer on the dearest call ends no run.
pub const TRIES: usize = 6;
/// The longest wait before a step is asked again, in seconds.
pub const LONGEST_WAIT: f64 = 60.0;

/// Whether `status` answers for now, not for good: no answer (0), a timeout, a rate limit, or a
/// server's failure, Anthropic's 529 and Cloudflare's edge among them.
pub fn busy(status: u16) -> bool {
    matches!(status, 0 | 408 | 429 | 500..=599)
}

/// Who writes the deck: a provider, the model the user picked from those their key lists, and
/// where its API is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub provider: Provider,
    pub model: String,
    /// The API's address, where it is not the provider's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Whether a browser makes the calls.
    #[serde(default)]
    pub browser: bool,
}

/// The deck's narrative: its arc, why, its executive summary, and its slides, as Narrato writes
/// them; and the brief it was written for, its sources left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Narrative {
    pub arc: String,
    pub arc_reasoning: String,
    pub executive_summary: String,
    pub slides: Vec<Slide>,
    /// Recovered from an answer cut short: what closed of it, enough for the length.
    pub truncated: bool,
    pub brief: Brief,
    /// The icons the slides' words name, each by the word (PLAN G.23), as the set that ships
    /// draws them: what the client found, which [`Generation::give_icons`] hands over. A word with
    /// none has no icon.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub icons: std::collections::BTreeMap<String, Icon>,
}

/// An icon a slide's word names (PLAN G.23): its name in its set, and its outline on its view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub name: String,
    /// SVG path data.
    pub path: String,
    /// The box of the path's own coordinates it is drawn on, `[x, y, w, h]`.
    pub view: [f64; 4],
}

impl Narrative {
    /// Its arc's definition.
    pub fn narrative_arc(&self) -> &'static arcs::NarrativeArc {
        arcs::arc(&self.arc)
    }

    /// The words its slides name icons by (PLAN G.23), each once, as written but trimmed and in
    /// lower case: what [`Narrative::icons`] keys the icons found by. One with no letter or digit
    /// (`-`, `—`, `$`, an emoji), which a model writes for an item it cannot picture, names none.
    pub fn icon_words(&self) -> Vec<String> {
        let mut words: Vec<String> = Vec::new();
        for word in self.slides.iter().flat_map(|s| &s.icons).map(|w| w.trim().to_lowercase()) {
            if word.chars().any(char::is_alphanumeric) && !words.contains(&word) {
                words.push(word);
            }
        }
        words
    }
}

/// Why a deck could not be generated.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NarratoError {
    #[error("{0}")]
    Brief(#[from] BriefError),
    /// What the provider said: a status and its words, a refusal, a block.
    #[error("{0}")]
    Provider(String),
    /// An answer that does not read as a deck.
    #[error("{0}")]
    Answer(String),
    /// Too few slides came back to be a deck of the length asked.
    #[error("the model wrote {delivered} slides, and a deck of this length has at least {minimum}")]
    Incomplete { delivered: usize, minimum: usize },
    #[error("the deck is written: nothing more is asked")]
    Done,
}

/// Where a generation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    /// The arc is to be chosen.
    Arc,
    /// The slides are to be written.
    Slides,
    /// The narrative is written.
    Done,
}

/// One deck's generation, step by step: the client asks for each [`Generation::request`],
/// makes it, and hands the response to [`Generation::answer`], until it is done.
#[derive(Debug, Clone)]
pub struct Generation {
    brief: Brief,
    settings: Settings,
    step: Step,
    arc: Option<ArcChoice>,
    /// Why the arc was chosen by the heuristic, where the model's choice did not come back.
    fallback: Option<String>,
    narrative: Option<Narrative>,
    /// How many times the step was asked and answered for now, not for good.
    tried: usize,
    /// How long to wait before the step is asked again, where its last answer was for now.
    again: Option<f64>,
    /// The slides as the model wrote them, the text [`Generation::written`] read.
    written: Option<String>,
}

impl Generation {
    /// A generation of `brief`, read as Narrato reads one, by `settings`' model.
    pub fn new(brief: Brief, settings: Settings) -> Result<Generation, NarratoError> {
        Ok(Generation {
            brief: brief.validated()?,
            settings,
            step: Step::Arc,
            arc: None,
            fallback: None,
            narrative: None,
            tried: 0,
            again: None,
            written: None,
        })
    }

    pub fn brief(&self) -> &Brief {
        &self.brief
    }

    pub fn step(&self) -> Step {
        self.step
    }

    /// The slides the deck is asked for at least.
    pub fn target(&self) -> usize {
        brief::slide_count(&self.brief.length)
    }

    /// The arc chosen, once it is.
    pub fn arc(&self) -> Option<&ArcChoice> {
        self.arc.as_ref()
    }

    /// Why the heuristic chose the arc, where the model's choice did not come back.
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    pub fn narrative(&self) -> Option<&Narrative> {
        self.narrative.as_ref()
    }

    /// How long to wait, in seconds, before the step is asked again ([`Generation::request`]
    /// gives the same request): where the last answer was for now, not for good. None where
    /// it was read, and after the last of [`TRIES`].
    pub fn again(&self) -> Option<f64> {
        self.again
    }

    /// How many times the step has been asked and answered for now, not for good.
    pub fn tried(&self) -> usize {
        self.tried
    }

    /// The slides as the model wrote them, the text [`Generation::written`] read, once it has:
    /// what a client keeps so a run that fails after them resumes from it, as `--slides` takes
    /// it, with no call paid for again (narrative REL-2).
    pub fn written_text(&self) -> Option<&str> {
        self.written.as_deref()
    }

    /// The words the narrative's slides name icons by ([`Narrative::icon_words`]); none before
    /// the narrative is written. The client finds each in the set that ships and hands back what
    /// it found ([`Generation::give_icons`]).
    pub fn icon_words(&self) -> Vec<String> {
        self.narrative.as_ref().map(Narrative::icon_words).unwrap_or_default()
    }

    /// The icons the client found for [`Generation::icon_words`], each by its word, kept with the
    /// narrative for the deck it makes (PLAN G.23).
    pub fn give_icons(&mut self, icons: std::collections::BTreeMap<String, Icon>) {
        if let Some(narrative) = &mut self.narrative {
            narrative.icons = icons;
        }
    }

    pub fn into_narrative(self) -> Option<Narrative> {
        self.narrative
    }

    /// Choose the arc without asking the model: `name`, one of the catalog's, or the heuristic's
    /// choice for the brief.
    pub fn choose_arc(&mut self, name: Option<&str>) {
        let choice = match name {
            Some(name) => {
                let arc = arcs::arc(name);
                ArcChoice {
                    selection: arc.arc.clone(),
                    reasoning: "Chosen by the user.".into(),
                    candidates: Vec::new(),
                }
            }
            None => heuristic::choose(&self.brief),
        };
        self.arc = Some(choice);
        self.step = Step::Slides;
    }

    /// The request the step asks for, `key` in its headers, `documents` (PDFs) beside the
    /// slides' prompt; none once the narrative is written. Arc selection sees the sources' first
    /// 500 characters, never their documents.
    pub fn request(&self, key: &str, documents: &[Document]) -> Option<Request> {
        use providers::Shape;
        let s = &self.settings;
        // Gemini is given the most it writes, as Vzestup gives it (`maxOutputTokens`).
        let room = |own: u32| if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { own };
        let (system, prompt, shape, max_tokens, temperature, documents) = match self.step {
            Step::Arc => (
                prompt::ARC_SELECT_SYSTEM,
                prompt::arc_selection(&self.brief),
                Shape::Arc,
                room(ARC_MAX_TOKENS),
                0.2,
                &[][..],
            ),
            Step::Slides => {
                let arc = arcs::arc(&self.arc.as_ref()?.selection);
                let text = prompt::generation(&self.brief, arc, self.target());
                (prompt::SLIDES_SYSTEM, text, Shape::Slides, room(SLIDES_MAX_TOKENS), 0.7, documents)
            }
            Step::Done => return None,
        };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system,
            prompt: &prompt,
            shape,
            // Vzestup holds Anthropic's arc selection to its shape by the rider alone.
            anthropic_schema: shape == Shape::Slides,
            max_tokens,
            temperature: Some(temperature),
            documents,
            pictures: &[],
        }))
    }

    /// The request that has `longs`, texts the deck set that run past their place, rewritten to
    /// fit ([`fit`]): Scaena's own call, made once the narrative is written; none before.
    pub fn fit_request(&self, key: &str, longs: &[fit::Long]) -> Option<Request> {
        let narrative = self.narrative.as_ref()?;
        let s = &self.settings;
        let prompt = fit::prompt(longs, narrative.brief.language.as_deref());
        let max_tokens = if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { FIT_MAX_TOKENS };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system: fit::FIT_SYSTEM,
            prompt: &prompt,
            shape: providers::Shape::Fit,
            anthropic_schema: true,
            max_tokens,
            temperature: Some(0.2),
            documents: &[],
            pictures: &[],
        }))
    }

    /// The request that shows the model the deck as it is drawn, `slides` each with what it may
    /// take ([`look::Seen`]) and `drawn` its picture, in their order, and asks what it would
    /// change ([`look`]): Scaena's own call, made once the narrative is written; none before, or
    /// for no slide.
    pub fn look_request(&self, key: &str, slides: &[look::Seen], drawn: &[Picture]) -> Option<Request> {
        let narrative = self.narrative.as_ref()?;
        if slides.is_empty() || slides.len() != drawn.len() {
            return None;
        }
        let s = &self.settings;
        let count = narrative.slides.len();
        let pictures: Vec<Picture> = (slides.iter().zip(drawn))
            .map(|(seen, d)| Picture { caption: look::caption(seen, count), ..d.clone() })
            .collect();
        let brief = &narrative.brief;
        let prompt = look::prompt(&brief.topic, &brief.audience, count, brief.language.as_deref());
        let max_tokens = if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { LOOK_MAX_TOKENS };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system: look::LOOK_SYSTEM,
            prompt: &prompt,
            shape: providers::Shape::Look,
            anthropic_schema: true,
            max_tokens,
            temperature: Some(0.3),
            documents: &[],
            pictures: &pictures,
        }))
    }

    /// The changes the response to [`Generation::look_request`] asks for, each one `slides`
    /// may take.
    pub fn look_answer(
        &self,
        status: u16,
        status_text: &str,
        body: &str,
        slides: &[look::Seen],
    ) -> Result<Vec<look::Change>, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        let read = self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
        look::read(&read.text, slides).map_err(NarratoError::Answer)
    }

    /// The rewrites the response to [`Generation::fit_request`] gives: each a text asked about,
    /// shorter than it was.
    pub fn fit_answer(
        &self,
        status: u16,
        status_text: &str,
        body: &str,
        longs: &[fit::Long],
    ) -> Result<Vec<(String, String)>, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        let read = self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
        fit::read(&read.text, longs).map_err(NarratoError::Answer)
    }

    /// Read the response to [`Generation::request`]: `status` 0 where it never came back, and
    /// `after` its `Retry-After`, where it has one. An answer for now, not for good (`busy`: no
    /// answer, a timeout, a rate limit, a server's failure) leaves the step standing to be asked
    /// again, after the wait [`Generation::again`] says, up to [`TRIES`] times in all (narrative
    /// REL-1). After the last, or any other failure, arc selection falls back to the heuristic,
    /// saying why; the slides that fail are an error, and the step stands to be asked again.
    pub fn answer(
        &mut self,
        status: u16,
        status_text: &str,
        body: &str,
        after: Option<&str>,
    ) -> Result<Step, NarratoError> {
        if self.step == Step::Done {
            return Err(NarratoError::Done);
        }
        if busy(status) && self.tried + 1 < TRIES {
            self.tried += 1;
            self.again = Some(providers::wait(after, body, self.tried));
            return Ok(self.step);
        }
        self.again = None;
        let tried = std::mem::take(&mut self.tried) + 1;
        let asked = |why: String| if tried > 1 { format!("{why} (asked {tried} times)") } else { why };
        match self.step {
            Step::Arc => {
                let chosen = match status {
                    0 => Err(format!("the request did not complete: {status_text}")),
                    _ => {
                        self.settings.provider.read_answer(status, status_text, body).and_then(|a| answer::arc(&a.text))
                    }
                };
                match chosen {
                    Ok(choice) => self.arc = Some(choice),
                    Err(why) => {
                        self.arc = Some(heuristic::choose(&self.brief));
                        self.fallback = Some(asked(why));
                    }
                }
                self.step = Step::Slides;
                Ok(self.step)
            }
            Step::Slides => {
                if status == 0 {
                    return Err(NarratoError::Provider(asked(format!("the request did not complete: {status_text}"))));
                }
                let read = (self.settings.provider.read_answer(status, status_text, body))
                    .map_err(|why| NarratoError::Provider(asked(why)))?;
                self.written(&read.text, read.stop == Stop::Length)
            }
            Step::Done => Err(NarratoError::Done),
        }
    }

    /// The slides as the model wrote them, `text`, the JSON the slides' request asks for, cut
    /// short where `stopped` says the model ran out of room: what [`Generation::answer`] reads
    /// from an answer, handed over by a client that read the answer itself, or written elsewhere.
    /// The arc is chosen first; too few slides for the length asked are refused.
    pub fn written(&mut self, text: &str, stopped: bool) -> Result<Step, NarratoError> {
        let arc = match (self.step, &self.arc) {
            (Step::Slides, Some(arc)) => arc.clone(),
            (Step::Done, _) => return Err(NarratoError::Done),
            _ => return Err(NarratoError::Answer("the arc is chosen before the slides are written".into())),
        };
        let written = answer::slides(text).map_err(NarratoError::Answer)?;
        if let Some(minimum) = brief::minimum_slides(&self.brief.length)
            && written.slides.len() < minimum
        {
            return Err(NarratoError::Incomplete { delivered: written.slides.len(), minimum });
        }
        self.written = Some(text.to_string());
        self.narrative = Some(Narrative {
            arc: arc.selection,
            arc_reasoning: arc.reasoning,
            executive_summary: written.executive_summary,
            slides: written.slides,
            truncated: written.recovered || stopped,
            brief: Brief { source_content: None, ..self.brief.clone() },
            icons: Default::default(),
        });
        self.step = Step::Done;
        Ok(self.step)
    }
}
