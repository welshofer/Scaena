//! # scaena-chat
//!
//! The assistant's conversation (ADR-0022, PLAN 3.6, SPEC §11): the user's question, with their
//! own key, to the provider they chose, and each tool the model calls run on the bundle, until
//! the model answers. It is what the browser's assistant does in TypeScript
//! (`web/src/assistant/`), for a client that is not a page: the Mac.
//!
//! It makes no HTTP call, reads no clock, and keeps no key. A [`Chat`] gives the client each
//! request to make ([`Chat::request`]), reads the answer it hands back ([`Chat::answer`]), and
//! takes the results of the calls the client ran on the session ([`Chat::results`]), until
//! the model answers without a call or the question has taken its rounds.

mod prompt;
mod providers;
mod tools;

pub use prompt::{Characters, Client, Facts, Listed, Seeing, Selected, seen, system, system_of};
pub use providers::{Ask, Provider, Request, Stop, Turn, Usage, inlined};
pub use tools::{RESOURCE_READ, resource_read, summary, tools};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Why the conversation cannot go on.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ChatError {
    /// What the provider said, or that it said nothing that reads.
    #[error("{0}")]
    Provider(String),
}

/// A tool the model may call: its name, what it does, and its arguments as JSON Schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

/// A call the model made: its id (the provider's, or one made up where it gives none), the
/// tool, and the arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub args: Value,
}

/// What a tool returned: its result as JSON, whether it failed, and a PNG, base64, for a frame
/// it drew.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub json: String,
    pub error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
}

/// The conversation, in no provider's form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        text: String,
    },
    Assistant {
        text: String,
        calls: Vec<Call>,
        /// The answer's parts as they came, where the provider needs them back (Gemini's).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Value>,
    },
    Tool {
        results: Vec<ToolResult>,
    },
}

/// The conversation as the model is sent it: the newest `frames` frames, and the older ones
/// named in their place, since each costs as much as a page of text.
pub fn recent(conversation: &[Message], frames: usize) -> Vec<Message> {
    let mut kept = 0;
    let mut out: Vec<Message> = conversation.to_vec();
    for m in out.iter_mut().rev() {
        let Message::Tool { results } = m else { continue };
        for r in results.iter_mut().rev() {
            if r.png.is_none() {
                continue;
            }
            if kept < frames {
                kept += 1;
                continue;
            }
            kept += 1;
            r.png = None;
            r.json = format!("{}\n(The frame is no longer shown: render it again to see it.)", r.json);
        }
    }
    out
}

/// One conversation with a model, kept between questions until the client forgets it.
#[derive(Debug, Clone)]
pub struct Chat {
    pub provider: Provider,
    pub model: String,
    /// The API's address; the provider's own where none is given.
    pub base: Option<String>,
    pub system: String,
    pub tools: Vec<Tool>,
    /// Whether a browser asks (Anthropic's opt-in header).
    pub browser: bool,
    pub conversation: Vec<Message>,
    /// The most rounds of calls one question may take.
    pub rounds: usize,
    /// The most frames the model is sent again: older ones are named, not shown.
    pub frames: usize,
    /// The rounds of calls the question asked last has taken.
    taken: usize,
}

/// What the model's answer comes to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "next", rename_all = "lowercase")]
pub enum Next {
    /// The calls to run on the bundle, then hand back with [`Chat::results`].
    Calls { text: String, calls: Vec<Call>, usage: Option<Usage> },
    /// The answer: the model is done, ran out of room, or the question has taken its rounds.
    Done { text: String, stop: Stop, usage: Option<Usage> },
}

impl Chat {
    pub fn new(provider: Provider, model: &str, system: String, tools: Vec<Tool>) -> Chat {
        Chat {
            provider,
            model: model.to_string(),
            base: None,
            system,
            tools,
            browser: false,
            conversation: Vec::new(),
            rounds: 32,
            frames: 2,
            taken: 0,
        }
    }

    /// Ask `text`, begun with what the editor shows ([`seen`]).
    pub fn ask(&mut self, text: &str) {
        self.conversation.push(Message::User { text: text.to_string() });
        self.taken = 0;
    }

    /// The request for the model's next turn, `key` in its headers.
    pub fn request(&self, key: &str) -> Request {
        let messages = recent(&self.conversation, self.frames);
        self.provider.turn(&Ask {
            model: &self.model,
            key,
            base: self.base.as_deref(),
            system: &self.system,
            tools: &self.tools,
            messages: &messages,
            browser: self.browser,
        })
    }

    /// Read the answer to [`Chat::request`]: its turn joins the conversation, and says what
    /// comes next. A provider's refusal, or an answer that does not read, is an error, and the
    /// conversation is as it was.
    pub fn answer(&mut self, status: u16, status_text: &str, body: &str) -> Result<Next, ChatError> {
        let turn = self.provider.read_turn(status, status_text, body)?;
        self.conversation.push(Message::Assistant {
            text: turn.text.clone(),
            calls: turn.calls.clone(),
            raw: turn.raw,
        });
        Ok(if turn.calls.is_empty() {
            Next::Done { text: turn.text, stop: turn.stop, usage: turn.usage }
        } else {
            Next::Calls { text: turn.text, calls: turn.calls, usage: turn.usage }
        })
    }

    /// The results of the calls the last answer made, every call answered, in its order:
    /// whether the model may be asked again, or the question has taken its rounds.
    pub fn results(&mut self, results: Vec<ToolResult>) -> bool {
        self.conversation.push(Message::Tool { results });
        self.taken += 1;
        self.taken < self.rounds
    }

    /// Start again: the next question is the first.
    pub fn forget(&mut self) {
        self.conversation.clear();
        self.taken = 0;
    }
}
