//! The assistant's model, with the user's own key (SPEC §11, ADR-0006): Anthropic's Messages
//! API, OpenAI's Chat Completions, and Gemini's generateContent. Each adapter turns one
//! conversation, kept in no provider's form, into its provider's request, and its answer back,
//! as the browser's adapters do (`web/src/assistant/providers.ts`): the record of their
//! exchanges, `tests/assistant/exchanges.json`, holds the two together (ADR-0022). No model is
//! named here: each lists the models the key can use, and the user picks one.

use crate::{Call, ChatError, Message, Tool, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// An HTTP request for the client to make: what each provider is sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    /// JSON; none for a GET.
    pub body: Option<Value>,
}

/// Why a model stopped: it waits for its calls' results, its answer is done, it ran out of
/// room, or another reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stop {
    Tools,
    End,
    Length,
    Other,
}

/// Tokens in and out, as the provider counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

/// One answer of the model: what it said, the tools it calls, and why it stopped. `raw` is the
/// answer's parts, kept to send back as they came where the provider asks for that (Gemini's
/// thought signatures).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub text: String,
    pub calls: Vec<Call>,
    pub stop: Stop,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// What a turn is asked with.
#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    pub model: &'a str,
    pub key: &'a str,
    /// The API's address; the provider's own where none is given.
    pub base: Option<&'a str>,
    pub system: &'a str,
    pub tools: &'a [Tool],
    pub messages: &'a [Message],
    /// Whether a browser asks: Anthropic takes a browser's call only with its opt-in header.
    pub browser: bool,
}

/// A provider of models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    Openai,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Anthropic, Provider::Openai, Provider::Gemini];

    /// Its name, for people.
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::Openai => "OpenAI",
            Provider::Gemini => "Gemini",
        }
    }

    /// The API's own address.
    pub fn base(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::Openai => "https://api.openai.com",
            Provider::Gemini => "https://generativelanguage.googleapis.com",
        }
    }

    /// The request that lists the models `key` can use.
    pub fn models(self, key: &str, base: Option<&str>, browser: bool) -> Request {
        let (path, headers) = match self {
            Provider::Anthropic => ("/v1/models?limit=100", anthropic_headers(key, browser)),
            Provider::Openai => ("/v1/models", headers([("authorization", format!("Bearer {key}"))])),
            Provider::Gemini => ("/v1beta/models?pageSize=1000", headers([("x-goog-api-key", key.to_string())])),
        };
        Request { method: "GET".into(), url: at(base, self.base(), path), headers, body: None }
    }

    /// The models an answer to [`Provider::models`] lists.
    pub fn read_models(self, status: u16, status_text: &str, body: &str) -> Result<Vec<String>, ChatError> {
        let body = read(status, status_text, body)?;
        let listed = |key: &str| body.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
        let id = |m: &Value| m.get("id").and_then(Value::as_str).map(str::to_string);
        Ok(match self {
            Provider::Anthropic => listed("data").iter().filter_map(id).collect(),
            Provider::Openai => {
                let mut ids: Vec<String> = listed("data").iter().filter_map(id).collect();
                ids.sort_unstable();
                ids
            }
            Provider::Gemini => (listed("models").iter())
                .filter(|m| {
                    m.get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .is_none_or(|ways| ways.iter().any(|w| w == "generateContent"))
                })
                .filter_map(|m| m.get("name").and_then(Value::as_str))
                .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
                .collect(),
        })
    }

    /// The request for the model's next turn in `ask`'s conversation.
    pub fn turn(self, ask: &Ask) -> Request {
        match self {
            Provider::Anthropic => anthropic(ask),
            Provider::Openai => openai(ask),
            Provider::Gemini => gemini(ask),
        }
    }

    /// The turn an answer to [`Provider::turn`] gives: streamed, as the request asks, or whole.
    pub fn read_turn(self, status: u16, status_text: &str, body: &str) -> Result<Turn, ChatError> {
        let body = self.read(status, status_text, body)?;
        Ok(match self {
            Provider::Anthropic => anthropic_turn(&body),
            Provider::Openai => openai_turn(&body),
            Provider::Gemini => gemini_turn(&body),
        })
    }
}

impl Provider {
    /// [`read`] as this provider's answer. A streamed answer (narrative ATTR-3) is assembled into
    /// the one the provider would have sent whole; an error it carried mid-stream, as Anthropic's
    /// `event: error` does when it is overloaded, is a refusal with that error's status (529 for
    /// overloaded, 500 otherwise), so the client says it as it says one that came as a status.
    fn read(self, status: u16, status_text: &str, text: &str) -> Result<Value, ChatError> {
        if (200..300).contains(&status) && crate::stream::is_sse(text) {
            let answer = crate::stream::assembled(self, text);
            if let Some(error) = answer.get("error") {
                let overloaded = error.get("type").and_then(Value::as_str) == Some("overloaded_error");
                let (status, status_text) = if overloaded { (529, "Overloaded") } else { (500, "Error") };
                let said = error.get("message").and_then(Value::as_str).unwrap_or_default();
                return Err(ChatError::Provider(format!("{status} {status_text}: {said}").trim().to_string()));
            }
            return Ok(answer);
        }
        read(status, status_text, text)
    }
}

/// A provider's answer as JSON, or why it gave none: its own message where it sends one.
fn read(status: u16, status_text: &str, text: &str) -> Result<Value, ChatError> {
    let body: Option<Value> = if text.is_empty() { Some(json!({})) } else { serde_json::from_str(text).ok() };
    if !(200..300).contains(&status) {
        let said = (body.as_ref())
            .and_then(|b| b.pointer("/error/message").or_else(|| b.get("message")))
            .and_then(Value::as_str)
            .map_or_else(|| first(text, 300), str::to_string);
        return Err(ChatError::Provider(format!("{status} {status_text}: {said}").trim().to_string()));
    }
    body.ok_or_else(|| ChatError::Provider(format!("not JSON: {}", first(text, 300))))
}

/// The first `n` characters of `text`, as JavaScript counts them (UTF-16), short of a pair it
/// would split.
pub(crate) fn first(text: &str, n: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for c in text.chars() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        out.push(c);
    }
    out
}

/// `base`, an API address (scaena-narrato's twin) typed beside a key, where a key may go to it: `https://` and a host, or
/// `http://` to this machine alone (`localhost`, `127.0.0.1`, `[::1]`, at any port), for a proxy
/// on it. Else why not: over plain `http` elsewhere the key would cross the network in the clear
/// with every call (lift-up 20261025 SEC-2).
pub fn checked_base(base: &str) -> Result<&str, String> {
    let base = base.trim();
    let host = |rest: &str| rest.split(['/', '?', '#']).next().unwrap_or_default().to_string();
    let lower = base.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://") {
        let host = host(rest);
        if !host.is_empty() && !host.contains('@') {
            return Ok(base);
        }
    } else if let Some(rest) = lower.strip_prefix("http://") {
        // The whole host, matched strictly, as the browser's `refusedBase` matches it: this
        // machine's name, then at most a port of digits, and nothing else, so no userinfo or
        // other text after the name reads as this machine (`http://[::1]@evil.example`).
        let host = host(rest);
        let local = ["localhost", "127.0.0.1", "[::1]"].iter().any(|name| {
            host.strip_prefix(name).is_some_and(|port| {
                port.is_empty() || port.strip_prefix(':').is_some_and(|d| d.bytes().all(|b| b.is_ascii_digit()))
            })
        });
        if local {
            return Ok(base);
        }
    }
    Err(format!(
        "{base}: an API address takes https://, or http:// to this machine (localhost, 127.0.0.1), so the key never crosses the network in the clear"
    ))
}

/// `path` at `base`, the provider's own address `fallback` where none is given. An address
/// [`checked_base`] refuses is never asked, the key never going to it: the clients refuse it
/// as it is typed, and this is the last guard, asking the provider's own instead.
fn at(base: Option<&str>, fallback: &str, path: &str) -> String {
    let base = base.filter(|b| !b.is_empty()).and_then(|b| checked_base(b).ok()).unwrap_or(fallback);
    format!("{}{path}", base.trim_end_matches('/'))
}

fn headers<const N: usize>(pairs: [(&str, String); N]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (name, value) in pairs {
        out.insert(name.to_string(), value);
    }
    out
}

/// `schema` with every `$ref` to its `$defs` written in place, and no `$schema`: what each
/// provider takes, whatever it makes of references.
pub fn inlined(schema: &Value) -> Value {
    let defs = schema.get("$defs").cloned().unwrap_or_else(|| json!({}));
    fn walk(v: &Value, defs: &Value, seen: &[String]) -> Value {
        match v {
            Value::Array(items) => Value::Array(items.iter().map(|x| walk(x, defs, seen)).collect()),
            Value::Object(o) => {
                if let Some(name) = o.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")) {
                    if seen.iter().any(|s| s == name) {
                        return json!({});
                    }
                    let mut deeper = seen.to_vec();
                    deeper.push(name.to_string());
                    let mut out = match walk(defs.get(name).unwrap_or(&Value::Null), defs, &deeper) {
                        Value::Object(m) => m,
                        _ => Map::new(),
                    };
                    let mut rest = o.clone();
                    rest.remove("$ref");
                    if let Value::Object(m) = walk(&Value::Object(rest), defs, seen) {
                        out.extend(m);
                    }
                    return Value::Object(out);
                }
                let mut out = Map::new();
                for (k, x) in o {
                    if k != "$defs" && k != "$schema" {
                        out.insert(k.clone(), walk(x, defs, seen));
                    }
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
    walk(schema, &defs, &[])
}

/// Items with each run of one role kept together, as providers that alternate roles need: a
/// tool's results, then the user's next words, are one user turn.
fn runs(items: Vec<(&'static str, Vec<Value>)>) -> Vec<(&'static str, Vec<Value>)> {
    let mut out: Vec<(&'static str, Vec<Value>)> = Vec::new();
    for (role, parts) in items {
        match out.last_mut() {
            Some((last, kept)) if *last == role => kept.extend(parts),
            _ => out.push((role, parts)),
        }
    }
    out
}

fn args(call: &Call) -> Value {
    if call.args.is_null() { json!({}) } else { call.args.clone() }
}

/// A result's JSON as a value: what it says, or the text itself.
fn parsed(text: &str) -> Value {
    if text.is_empty() {
        return json!({});
    }
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

fn anthropic_headers(key: &str, browser: bool) -> BTreeMap<String, String> {
    let mut out = headers([("x-api-key", key.to_string()), ("anthropic-version", "2023-06-01".to_string())]);
    if browser {
        // A browser calls the API with the user's own key, which never leaves the page but for
        // Anthropic (ADR-0006).
        out.insert("anthropic-dangerous-direct-browser-access".into(), "true".into());
    }
    out
}

fn anthropic(ask: &Ask) -> Request {
    let items = ask.messages.iter().map(|m| match m {
        Message::User { text, png } => {
            let mut content = Vec::new();
            if let Some(png) = png {
                content.push(
                    json!({ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": png } }),
                );
            }
            content.push(json!({ "type": "text", "text": text }));
            ("user", content)
        }
        Message::Assistant { text, calls, .. } => {
            let mut content = if text.is_empty() { vec![] } else { vec![json!({ "type": "text", "text": text })] };
            for c in calls {
                content.push(json!({ "type": "tool_use", "id": c.id, "name": c.name, "input": args(c) }));
            }
            ("assistant", content)
        }
        Message::Tool { results } => ("user", results.iter().map(anthropic_result).collect()),
    });
    let messages: Vec<Value> =
        runs(items.collect()).into_iter().map(|(role, content)| json!({ "role": role, "content": content })).collect();
    let tools: Vec<Value> = (ask.tools.iter())
        .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": inlined(&t.schema) }))
        .collect();
    let mut body = json!({
        "model": ask.model,
        "max_tokens": 8192,
        // The system prompt is the same each turn: cached, it costs a tenth on every turn after the first.
        "system": [{ "type": "text", "text": ask.system, "cache_control": { "type": "ephemeral" } }],
        "messages": messages,
        // The answer as it is written, as server-sent events (narrative ATTR-3).
        "stream": true,
    });
    // A question asked once, with nothing to call, sends no tools (PLAN 2.101).
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    let mut headers = anthropic_headers(ask.key, ask.browser);
    headers.insert("content-type".into(), "application/json".into());
    Request {
        method: "POST".into(),
        url: at(ask.base, Provider::Anthropic.base(), "/v1/messages"),
        headers,
        body: Some(body),
    }
}

fn anthropic_result(r: &ToolResult) -> Value {
    let mut content = vec![json!({ "type": "text", "text": r.json })];
    if let Some(png) = &r.png {
        content
            .push(json!({ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": png } }));
    }
    json!({ "type": "tool_result", "tool_use_id": r.id, "is_error": r.error, "content": content })
}

fn anthropic_turn(answer: &Value) -> Turn {
    let content = answer.get("content").and_then(Value::as_array).cloned().unwrap_or_default();
    let kind = |c: &Value| c.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
    let text: Vec<&str> = (content.iter())
        .filter(|c| kind(c) == "text")
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .filter(|t| !t.is_empty())
        .collect();
    let calls = (content.iter())
        .filter(|c| kind(c) == "tool_use")
        .map(|c| Call {
            id: c.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
            name: c.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            args: c.get("input").cloned().unwrap_or(Value::Null),
        })
        .collect();
    let stop = match answer.get("stop_reason").and_then(Value::as_str) {
        Some("tool_use") => Stop::Tools,
        Some("end_turn") => Stop::End,
        Some("max_tokens") => Stop::Length,
        _ => Stop::Other,
    };
    let usage =
        answer.get("usage").map(|u| Usage { input: count(u, "input_tokens"), output: count(u, "output_tokens") });
    Turn { text: text.join("\n\n"), calls, stop, raw: None, usage }
}

fn count(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn openai(ask: &Ask) -> Request {
    let mut messages = vec![json!({ "role": "system", "content": ask.system })];
    for m in ask.messages {
        match m {
            Message::User { text, png: None } => messages.push(json!({ "role": "user", "content": text })),
            Message::User { text, png: Some(png) } => messages.push(json!({ "role": "user", "content": [
                { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{png}") } },
                { "type": "text", "text": text },
            ] })),
            Message::Assistant { text, calls, .. } => {
                let mut message =
                    json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
                if !calls.is_empty() {
                    let called: Vec<Value> = (calls.iter())
                        .map(|c| {
                            let arguments = args(c).to_string();
                            json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": arguments } })
                        })
                        .collect();
                    message["tool_calls"] = Value::Array(called);
                }
                messages.push(message);
            }
            Message::Tool { results } => {
                for r in results {
                    messages.push(json!({ "role": "tool", "tool_call_id": r.id, "content": r.json }));
                }
                // A tool's message holds text alone: the frames it drew follow as the user's.
                let frames: Vec<&ToolResult> = results.iter().filter(|r| r.png.is_some()).collect();
                if !frames.is_empty() {
                    let ids: Vec<&str> = frames.iter().map(|r| r.id.as_str()).collect();
                    let plural = if frames.len() > 1 { "s" } else { "" };
                    let mut content = vec![
                        json!({ "type": "text", "text": format!("The frame{plural} deck_render drew ({}):", ids.join(", ")) }),
                    ];
                    for r in &frames {
                        let url = format!("data:image/png;base64,{}", r.png.as_deref().unwrap_or_default());
                        content.push(json!({ "type": "image_url", "image_url": { "url": url } }));
                    }
                    messages.push(json!({ "role": "user", "content": content }));
                }
            }
        }
    }
    let tools: Vec<Value> = (ask.tools.iter())
        .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": inlined(&t.schema) } }))
        .collect();
    // The answer as it is written, as server-sent events, the usage in its last chunk (narrative ATTR-3).
    let mut body = json!({
        "model": ask.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    // OpenAI refuses an empty list of tools: a question asked once sends none (PLAN 2.101).
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    let headers =
        headers([("authorization", format!("Bearer {}", ask.key)), ("content-type", "application/json".to_string())]);
    Request {
        method: "POST".into(),
        url: at(ask.base, Provider::Openai.base(), "/v1/chat/completions"),
        headers,
        body: Some(body),
    }
}

fn openai_turn(answer: &Value) -> Turn {
    let choice = answer.pointer("/choices/0").cloned().unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));
    let calls: Vec<Call> = (message.get("tool_calls").and_then(Value::as_array).cloned().unwrap_or_default().iter())
        .map(|c| Call {
            id: c.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
            name: c.pointer("/function/name").and_then(Value::as_str).unwrap_or_default().to_string(),
            args: parsed(c.pointer("/function/arguments").and_then(Value::as_str).unwrap_or_default()),
        })
        .collect();
    // An answer that ran out of room says so, its calls or not: the last may be cut off (REL-1).
    let stop = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => Stop::Length,
        _ if !calls.is_empty() => Stop::Tools,
        Some("stop") => Stop::End,
        _ => Stop::Other,
    };
    let usage =
        answer.get("usage").map(|u| Usage { input: count(u, "prompt_tokens"), output: count(u, "completion_tokens") });
    let text = message.get("content").and_then(Value::as_str).unwrap_or_default().to_string();
    Turn { text, calls, stop, raw: None, usage }
}

/// The start of an id Gemini did not give a call: one of ours, which it is not sent back.
const LOCAL: &str = "local-";

fn gemini(ask: &Ask) -> Request {
    let items = ask.messages.iter().map(|m| match m {
        Message::User { text, png } => {
            let mut parts = Vec::new();
            if let Some(png) = png {
                parts.push(json!({ "inlineData": { "mimeType": "image/png", "data": png } }));
            }
            parts.push(json!({ "text": text }));
            ("user", parts)
        }
        Message::Assistant { text, calls, raw } => {
            if let Some(Value::Array(parts)) = raw {
                return ("model", parts.clone());
            }
            let mut parts = if text.is_empty() { vec![] } else { vec![json!({ "text": text })] };
            for c in calls {
                parts.push(json!({ "functionCall": { "name": c.name, "args": args(c) } }));
            }
            ("model", parts)
        }
        Message::Tool { results } => {
            let mut parts: Vec<Value> = (results.iter())
                .map(|r| {
                    let response = if r.error {
                        json!({ "error": parsed(&r.json) })
                    } else {
                        json!({ "result": parsed(&r.json) })
                    };
                    let mut answered = Map::new();
                    if !r.id.starts_with(LOCAL) {
                        answered.insert("id".into(), json!(r.id));
                    }
                    answered.insert("name".into(), json!(r.name));
                    answered.insert("response".into(), response);
                    json!({ "functionResponse": answered })
                })
                .collect();
            for r in results {
                if let Some(png) = &r.png {
                    parts.push(json!({ "inlineData": { "mimeType": "image/png", "data": png } }));
                }
            }
            ("user", parts)
        }
    });
    let contents: Vec<Value> =
        runs(items.collect()).into_iter().map(|(role, parts)| json!({ "role": role, "parts": parts })).collect();
    let declarations: Vec<Value> = (ask.tools.iter())
        .map(|t| json!({ "name": t.name, "description": t.description, "parametersJsonSchema": inlined(&t.schema) }))
        .collect();
    let mut body = json!({
        "systemInstruction": { "parts": [{ "text": ask.system }] },
        "contents": contents,
    });
    // A question asked once, with nothing to call, declares no functions (PLAN 2.101).
    if !declarations.is_empty() {
        body["tools"] = json!([{ "functionDeclarations": declarations }]);
    }
    let model = ask.model.strip_prefix("models/").unwrap_or(ask.model);
    // The answer as it is written, as server-sent events (narrative ATTR-3).
    let path = format!("/v1beta/models/{}:streamGenerateContent?alt=sse", component(model));
    let headers = headers([("x-goog-api-key", ask.key.to_string()), ("content-type", "application/json".to_string())]);
    Request { method: "POST".into(), url: at(ask.base, Provider::Gemini.base(), &path), headers, body: Some(body) }
}

fn gemini_turn(answer: &Value) -> Turn {
    let candidate = answer.pointer("/candidates/0").cloned().unwrap_or_else(|| json!({}));
    let parts = candidate.pointer("/content/parts").and_then(Value::as_array).cloned().unwrap_or_default();
    let calls: Vec<Call> = (parts.iter().enumerate())
        .filter_map(|(i, p)| {
            let call = p.get("functionCall")?;
            let name = call.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let id =
                call.get("id").and_then(Value::as_str).map_or_else(|| format!("{LOCAL}{i}-{name}"), str::to_string);
            Some(Call { id, name, args: call.get("args").cloned().unwrap_or_else(|| json!({})) })
        })
        .collect();
    let text: String = (parts.iter())
        .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    let stop = match candidate.get("finishReason").and_then(Value::as_str) {
        Some("MAX_TOKENS") => Stop::Length,
        _ if !calls.is_empty() => Stop::Tools,
        Some("STOP") => Stop::End,
        _ => Stop::Other,
    };
    let usage = (answer.get("usageMetadata"))
        .map(|u| Usage { input: count(u, "promptTokenCount"), output: count(u, "candidatesTokenCount") });
    Turn { text, calls, stop, raw: Some(Value::Array(parts)), usage }
}

/// `text` as `encodeURIComponent` writes it into a URL's path.
fn component(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key goes over plain `http` to this machine alone (lift-up 20261025 SEC-2): every other
    /// address but `https://` is refused, and one refused is never asked.
    #[test]
    fn a_key_never_goes_over_plain_http_off_this_machine() {
        // The cases the browser's `refusedBase` is held to as well (`web/exchanges.mjs`).
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/assistant/bases.json")).expect("the cases");
        for good in cases["good"].as_array().unwrap().iter().map(|g| g.as_str().unwrap()) {
            assert_eq!(checked_base(good), Ok(good), "{good}");
        }
        for bad in cases["bad"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()) {
            let why = checked_base(bad).unwrap_err();
            assert!(why.contains("https://"), "{bad}: {why}");
        }
        assert_eq!(
            at(Some("http://evil.example"), "https://api.anthropic.com", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            at(Some("http://localhost:4000/"), "https://x", "/v1/messages"),
            "http://localhost:4000/v1/messages"
        );
    }
}
