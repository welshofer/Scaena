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
pub mod heuristic;
mod js;
pub mod layouts;
pub mod prompt;
pub mod providers;
pub mod schema;

pub use answer::{ChartData, Series, Slide};
pub use brief::{Brief, BriefError};
pub use heuristic::{ArcChoice, Candidate};
pub use providers::{Answer, Document, Provider, Request, Stop, Usage};

use serde::{Deserialize, Serialize};

/// Room for arc selection's answer, thinking included (Vzestup's `ARC_SELECT_MAX_TOKENS`).
pub const ARC_MAX_TOKENS: u32 = 16_000;
/// Room for the slides, thinking included: a 45-slide deck is some 30 KB of JSON (Vzestup's
/// `SLIDES_MAX_TOKENS`).
pub const SLIDES_MAX_TOKENS: u32 = 64_000;
/// Room for Gemini's answers, either: the most it writes (Vzestup's `maxOutputTokens`).
pub const GEMINI_MAX_TOKENS: u32 = 65_536;

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
}

impl Narrative {
    /// Its arc's definition.
    pub fn narrative_arc(&self) -> &'static arcs::NarrativeArc {
        arcs::arc(&self.arc)
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
        }))
    }

    /// Read the response to [`Generation::request`]: `status` 0 where it never came back. Arc
    /// selection that fails in any way falls back to the heuristic, saying why; the slides that
    /// fail are an error, and the step stands to be asked again.
    pub fn answer(&mut self, status: u16, status_text: &str, body: &str) -> Result<Step, NarratoError> {
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
                        self.fallback = Some(why);
                    }
                }
                self.step = Step::Slides;
                Ok(self.step)
            }
            Step::Slides => {
                if status == 0 {
                    return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
                }
                let read =
                    self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
                let written = answer::slides(&read.text).map_err(NarratoError::Answer)?;
                let truncated = written.recovered || read.stop == Stop::Length;
                if let Some(minimum) = brief::minimum_slides(&self.brief.length)
                    && written.slides.len() < minimum
                {
                    return Err(NarratoError::Incomplete { delivered: written.slides.len(), minimum });
                }
                let arc = self.arc.clone().expect("chosen before the slides");
                self.narrative = Some(Narrative {
                    arc: arc.selection,
                    arc_reasoning: arc.reasoning,
                    executive_summary: written.executive_summary,
                    slides: written.slides,
                    truncated,
                    brief: Brief { source_content: None, ..self.brief.clone() },
                });
                self.step = Step::Done;
                Ok(self.step)
            }
            Step::Done => Err(NarratoError::Done),
        }
    }
}
