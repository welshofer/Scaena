//! The assistant's conversation for Swift (PLAN 3.6, ADR-0022): `scaena-chat`'s steps over a
//! session, each call the model makes run on it as the browser's assistant runs one
//! (`web/src/assistant/index.ts`): by `agent:` and the model's name, a resource read from what
//! the MCP server serves, and a frame sent back as a PNG. Swift makes each HTTP request with
//! `URLSession`; the key it hands in builds one request's headers and is kept nowhere here.

use crate::{Failure, guarded, said};
use base64::Engine;
use scaena_chat::{Call, Chat, Client, Facts, Message, Provider, RESOURCE_READ, Request, Seeing, ToolResult};
use scaena_session::Session;
use scaena_session::assistant::{Caller, TOOLS, failure};
use serde_json::{Value, json};

/// What a call the user stopped before it ran is answered: every call has an answer.
const STOPPED: &str = "the user stopped the assistant";

/// A conversation with a model, and the results of the calls of its last answer run so far.
pub struct Asking {
    chat: Chat,
    ran: Vec<ToolResult>,
}

impl Asking {
    /// A conversation with `args`' model: `{provider, model, base?}`.
    pub fn new(args: &Value) -> Result<Asking, Failure> {
        let provider = provider(args)?;
        let model = (args.get("model").and_then(Value::as_str))
            .filter(|m| !m.is_empty())
            .ok_or_else(|| said("a conversation needs `model`, a string"))?;
        let tools = scaena_chat::tools(TOOLS).map_err(said)?;
        let mut chat = Chat::new(provider, model, String::new(), tools);
        chat.base = args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty()).map(str::to_string);
        Ok(Asking { chat, ran: Vec::new() })
    }

    /// One step of the conversation, by its name.
    pub fn call(&mut self, session: Option<&mut Session>, method: &str, args: &Value) -> Result<Value, Failure> {
        let arg = |key: &str| {
            args.get(key).and_then(Value::as_str).ok_or_else(|| said(format!("{method}: `{key}` is a string it needs")))
        };
        let session = session.ok_or_else(|| said(format!("{method}: the session is null")));
        Ok(match method {
            "ask" => {
                let s = session?;
                let seeing: Option<Seeing> = match args.get("seeing") {
                    None | Some(Value::Null) => None,
                    Some(seeing) => {
                        Some(serde_json::from_value(seeing.clone()).map_err(|e| said(format!("ask: `seeing`: {e}")))?)
                    }
                };
                // Told the deck as it is now, at each question, as the browser tells it.
                self.chat.system = scaena_chat::system(Client::App, &facts(s)?, &skills(s));
                let text = scaena_chat::seen(Client::App, seeing.as_ref()) + arg("text")?;
                self.chat.ask(&text);
                self.ran.clear();
                Value::Null
            }
            "request" => request(self.chat.request(arg("key")?)),
            "answer" => {
                let status = (args.get("status").and_then(Value::as_u64))
                    .and_then(|s| u16::try_from(s).ok())
                    .ok_or_else(|| said("answer: `status` is an HTTP status it needs"))?;
                let status_text = args.get("statusText").and_then(Value::as_str).unwrap_or_default();
                let next = self.chat.answer(status, status_text, arg("body")?).map_err(said)?;
                self.ran.clear();
                serde_json::to_value(next).map_err(said)?
            }
            "run" => {
                let call: Call = match args.get("index").and_then(Value::as_u64) {
                    // The call by its place in the last answer: its arguments as the model wrote them.
                    Some(index) => (self.calls().and_then(|calls| calls.get(index as usize)).cloned())
                        .ok_or_else(|| said(format!("run: the last answer made no call {index}")))?,
                    None => serde_json::from_value(args.get("call").cloned().unwrap_or_default())
                        .map_err(|e| said(format!("run: `index`, or `call` as {{id, name, args}}: {e}")))?,
                };
                let at = args.get("at").and_then(Value::as_str).and_then(scaena_session::store::seconds);
                let (result, ran) = run(session?, &self.chat.model, &call, at);
                self.ran.push(result);
                ran
            }
            "next" => {
                let results = self.answered().ok_or_else(|| said("next: the model's last answer made no calls"))?;
                json!(self.chat.results(results))
            }
            "use" => {
                // Another model, or provider, from the next request on: the conversation goes on.
                self.chat.provider = provider(args)?;
                self.chat.model = arg("model")?.to_string();
                self.chat.base = args.get("base").and_then(Value::as_str).filter(|b| !b.is_empty()).map(str::to_string);
                Value::Null
            }
            "forget" => {
                self.chat.forget();
                self.ran.clear();
                Value::Null
            }
            "conversation" => serde_json::to_value(&self.chat.conversation).map_err(said)?,
            _ => return Err(said(format!("`{method}` is not a step of a conversation"))),
        })
    }

    /// The calls of the last answer, where it made any and their results are not handed back.
    fn calls(&self) -> Option<&[Call]> {
        match self.chat.conversation.last() {
            Some(Message::Assistant { calls, .. }) if !calls.is_empty() => Some(calls),
            _ => None,
        }
    }

    /// The results of the last answer's calls, every one answered in its order: those not run,
    /// as stopped. None where the last answer made no calls.
    fn answered(&mut self) -> Option<Vec<ToolResult>> {
        let calls = self.calls()?.to_vec();
        let mut ran = std::mem::take(&mut self.ran);
        Some(
            calls
                .iter()
                .map(|call| match ran.iter().position(|r| r.id == call.id) {
                    Some(i) => ran.remove(i),
                    None => ToolResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        json: json!({ "message": STOPPED }).to_string(),
                        error: true,
                        png: None,
                    },
                })
                .collect(),
        )
    }
}

/// The models' providers, outside any conversation: `list`, the request for the `models` a key
/// can use, and `readModels`, the models its answer lists.
pub fn providers(method: &str, args: &Value) -> Result<Value, Failure> {
    let key = |name: &str| {
        args.get(name).and_then(Value::as_str).ok_or_else(|| said(format!("{method}: `{name}` is a string it needs")))
    };
    Ok(match method {
        "list" => (Provider::ALL.iter())
            .map(|p| json!({ "id": p, "name": p.name(), "base": p.base() }))
            .collect::<Vec<_>>()
            .into(),
        "models" => {
            let base = args.get("base").and_then(Value::as_str);
            request(provider(args)?.models(key("key")?, base, false))
        }
        "readModels" => {
            let status = (args.get("status").and_then(Value::as_u64))
                .and_then(|s| u16::try_from(s).ok())
                .ok_or_else(|| said("readModels: `status` is an HTTP status it needs"))?;
            let status_text = args.get("statusText").and_then(Value::as_str).unwrap_or_default();
            json!(provider(args)?.read_models(status, status_text, key("body")?).map_err(said)?)
        }
        _ => return Err(said(format!("`{method}` is not a call the providers answer"))),
    })
}

fn provider(args: &Value) -> Result<Provider, Failure> {
    serde_json::from_value(args.get("provider").cloned().unwrap_or_default())
        .map_err(|_| said("`provider` is anthropic, openai, or gemini"))
}

/// A request as Swift makes it: its body as the text sent.
fn request(r: Request) -> Value {
    json!({ "method": r.method, "url": r.url, "headers": r.headers, "body": r.body.map(|b| b.to_string()) })
}

/// The open deck, in a few facts: its title, states, formats, and theme.
fn facts(s: &Session) -> Result<Facts, Failure> {
    let deck = serde_json::to_value(s.deck()).map_err(said)?;
    Ok(Facts::of(&deck, s.states(), s.formats()))
}

/// The skills the bundle carries, by name (`skills/NAME/SKILL.md`).
fn skills(s: &Session) -> Vec<String> {
    (s.files().iter())
        .filter_map(|path| path.strip_prefix("skills/")?.strip_suffix("/SKILL.md"))
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .map(str::to_string)
        .collect()
}

/// Run `call` on the session as the model's agent, at `at`: its result for the model, and what
/// Swift shows of it: a line saying what it came to, the frame it drew, whether it changed the
/// deck, and the files it wrote beside the deck before and after, for an undo. A call that
/// fails says why to the model, as an MCP tool's error result does.
fn run(s: &mut Session, model: &str, call: &Call, at: Option<i64>) -> (ToolResult, Value) {
    let (json, error, png, edited, rewritten) = if call.name == RESOURCE_READ {
        let (json, error) = read(s, &call.args);
        (json, error, None, false, json!([]))
    } else {
        let args = if call.args.is_null() { json!({}) } else { call.args.clone() };
        let author = format!("agent:{model}");
        match guarded(|| s.tool(&call.name, args, Caller { author: &author, at }).map_err(|e| failure(&e))) {
            Ok(called) => {
                let png = (called.frame.as_ref())
                    .and_then(|frame| frame.to_png_fast().ok())
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
                let rewritten = serde_json::to_value(&called.rewritten).unwrap_or_else(|_| json!([]));
                (called.result, false, png, called.edited, rewritten)
            }
            Err(failure) => (failure.to_string(), true, None, false, json!([])),
        }
    };
    let ran = json!({
        "id": call.id,
        "name": call.name,
        "error": error,
        "summary": scaena_chat::summary(&call.name, &json, error),
        "json": json,
        "png": png,
        "edited": edited,
        "rewritten": rewritten,
    });
    (ToolResult { id: call.id.clone(), name: call.name.clone(), json, error, png }, ran)
}

/// `resource_read`: a resource's text, a skill the bundle carries, or the theme the deck is
/// drawn in; or why there is none.
fn read(s: &Session, args: &Value) -> (String, bool) {
    let Some(uri) = args.get("uri").and_then(Value::as_str) else {
        return (json!({ "message": "resource_read takes `uri`, a string" }).to_string(), true);
    };
    let skill = uri.strip_prefix("bundle://skills/").filter(|name| !name.is_empty() && !name.contains('/'));
    if uri == "bundle://theme" {
        // The theme the deck is drawn in, as theme_edit edits it (PLAN 2.61).
        if let Some(text) = s.theme_text().and_then(|held| held["text"].as_str().map(str::to_string)) {
            return (text, false);
        }
    } else if let Some(skill) = skill {
        if let Some(bytes) = s.file(&format!("skills/{skill}/SKILL.md")) {
            return (String::from_utf8_lossy(bytes).into_owned(), false);
        }
    } else if let Some(text) = scaena_resources::text(uri) {
        return (text, false);
    }
    let message = format!(
        "no resource `{uri}`: scaena://spec is the specification's index, and the system prompt lists the rest"
    );
    (json!({ "message": message }).to_string(), true)
}
