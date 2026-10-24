//! A model's answer as it is written (narrative ATTR-3): each provider streams its answer as
//! server-sent events, and this assembles them into the answer the provider would have sent
//! whole, so the readers in [`crate::providers`] read a streamed answer as they read one that
//! came at once; and it hands out the words as they arrive, for a panel to show. The browser's
//! `web/src/assistant/stream.ts` does the same, and the recorded exchanges hold the two together
//! (ADR-0022).
//!
//! A client feeds a [`Stream`] what it reads, in whatever pieces the network hands it, and takes
//! the text each piece completes; at the end, [`Stream::finish`] is the answer as JSON. A body
//! read whole goes through [`assembled`].

use crate::Provider;
use serde_json::{Map, Value, json};

/// Whether `body` is a stream of server-sent events, rather than one JSON value: a provider
/// answers a streamed request so, and answers an error, or an OpenAI-compatible server that
/// does not stream, with JSON.
pub fn is_sse(body: &str) -> bool {
    let first = body.trim_start();
    first.starts_with("event:") || first.starts_with("data:") || first.starts_with(':')
}

/// `body`, a provider's streamed answer read whole, as the answer it would have sent at once.
pub fn assembled(provider: Provider, body: &str) -> Value {
    let mut stream = Stream::new(provider);
    stream.feed(body);
    stream.finish()
}

/// One streamed answer being assembled.
#[derive(Debug, Clone)]
pub struct Stream {
    provider: Provider,
    /// What was fed and not yet read: the last line, which may be cut mid-way.
    pending: String,
    /// The event being read: its `event` field and its `data` lines.
    event: Option<String>,
    data: Vec<String>,
    /// An error the stream carried mid-way (Anthropic's `event: error`).
    error: Option<Value>,
    anthropic: Anthropic,
    openai: Openai,
    gemini: Gemini,
}

/// Anthropic's message, block by block.
#[derive(Debug, Clone, Default)]
struct Anthropic {
    message: Map<String, Value>,
    blocks: Vec<Block>,
}

/// One content block of Anthropic's answer: its text, or a tool's use with its input as JSON
/// text, built up delta by delta.
#[derive(Debug, Clone)]
struct Block {
    kind: String,
    id: String,
    name: String,
    text: String,
    json: String,
}

/// OpenAI's choice, delta by delta.
#[derive(Debug, Clone, Default)]
struct Openai {
    id: Option<Value>,
    content: Option<String>,
    calls: Vec<(String, String, String)>,
    finish: Option<Value>,
    usage: Option<Value>,
}

/// Gemini's candidate, chunk by chunk.
#[derive(Debug, Clone, Default)]
struct Gemini {
    parts: Vec<Value>,
    finish: Option<Value>,
    usage: Option<Value>,
}

impl Stream {
    pub fn new(provider: Provider) -> Stream {
        Stream {
            provider,
            pending: String::new(),
            event: None,
            data: Vec::new(),
            error: None,
            anthropic: Anthropic::default(),
            openai: Openai::default(),
            gemini: Gemini::default(),
        }
    }

    /// Read `chunk`, the next piece of the body: the words of the answer it completes, which a
    /// panel appends to what the model has said so far. A piece may end mid-line, or mid-event.
    pub fn feed(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        let mut said = String::new();
        while let Some(end) = self.pending.find('\n') {
            let line = self.pending[..end].trim_end_matches('\r').to_string();
            self.pending.drain(..=end);
            said.push_str(&self.line(&line));
        }
        said
    }

    /// One line of the stream: a field of the event being read, or the blank line that ends it.
    fn line(&mut self, line: &str) -> String {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return String::new();
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
        String::new()
    }

    /// The event read so far, taken: the words it adds.
    fn dispatch(&mut self) -> String {
        let event = self.event.take();
        let data = std::mem::take(&mut self.data).join("\n");
        if data.is_empty() {
            return String::new();
        }
        if data == "[DONE]" {
            return String::new();
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else { return String::new() };
        match self.provider {
            Provider::Anthropic => self.anthropic_event(event.as_deref(), &value),
            Provider::Openai => self.openai_chunk(&value),
            Provider::Gemini => self.gemini_chunk(&value),
        }
    }

    fn anthropic_event(&mut self, event: Option<&str>, v: &Value) -> String {
        let kind = event.or_else(|| v.get("type").and_then(Value::as_str)).unwrap_or_default();
        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let a = &mut self.anthropic;
        match kind {
            "message_start" => {
                if let Some(Value::Object(m)) = v.get("message") {
                    a.message = m.clone();
                }
            }
            "content_block_start" => {
                let block = v.get("content_block").cloned().unwrap_or_else(|| json!({}));
                let field = |k: &str| block.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                let made = Block {
                    kind: field("type"),
                    id: field("id"),
                    name: field("name"),
                    text: field("text"),
                    json: String::new(),
                };
                let said = if made.kind == "text" { made.text.clone() } else { String::new() };
                while a.blocks.len() < index {
                    a.blocks.push(Block {
                        kind: String::new(),
                        id: String::new(),
                        name: String::new(),
                        text: String::new(),
                        json: String::new(),
                    });
                }
                if index < a.blocks.len() {
                    a.blocks[index] = made;
                } else {
                    a.blocks.push(made);
                }
                return said;
            }
            "content_block_delta" => {
                let Some(block) = a.blocks.get_mut(index) else { return String::new() };
                let delta = v.get("delta").cloned().unwrap_or_else(|| json!({}));
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = delta.get("text").and_then(Value::as_str).unwrap_or_default();
                        block.text.push_str(text);
                        return text.to_string();
                    }
                    Some("input_json_delta") => {
                        block.json.push_str(delta.get("partial_json").and_then(Value::as_str).unwrap_or_default());
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(Value::Object(delta)) = v.get("delta") {
                    for (k, x) in delta {
                        a.message.insert(k.clone(), x.clone());
                    }
                }
                if let Some(Value::Object(usage)) = v.get("usage") {
                    let had = a.message.entry("usage").or_insert_with(|| json!({}));
                    if let Value::Object(had) = had {
                        for (k, x) in usage {
                            had.insert(k.clone(), x.clone());
                        }
                    }
                }
            }
            "error" => self.error = Some(v.clone()),
            _ => {}
        }
        String::new()
    }

    fn openai_chunk(&mut self, v: &Value) -> String {
        let o = &mut self.openai;
        if o.id.is_none() {
            o.id = v.get("id").cloned();
        }
        if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
            o.usage = Some(usage.clone());
        }
        if let Some(error) = v.get("error").filter(|e| !e.is_null()) {
            self.error = Some(json!({ "error": error }));
            return String::new();
        }
        let Some(choice) = v.pointer("/choices/0") else { return String::new() };
        if let Some(finish) = choice.get("finish_reason").filter(|f| !f.is_null()) {
            o.finish = Some(finish.clone());
        }
        let mut said = String::new();
        if let Some(delta) = choice.get("delta") {
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                o.content.get_or_insert_with(String::new).push_str(text);
                said.push_str(text);
            }
            for call in delta.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
                let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                while o.calls.len() <= index {
                    o.calls.push((String::new(), String::new(), String::new()));
                }
                let slot = &mut o.calls[index];
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    slot.0 = id.to_string();
                }
                if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                    slot.1.push_str(name);
                }
                if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str) {
                    slot.2.push_str(args);
                }
            }
        }
        said
    }

    fn gemini_chunk(&mut self, v: &Value) -> String {
        let g = &mut self.gemini;
        if let Some(usage) = v.get("usageMetadata") {
            g.usage = Some(usage.clone());
        }
        if let Some(error) = v.get("error").filter(|e| !e.is_null()) {
            self.error = Some(json!({ "error": error }));
            return String::new();
        }
        let Some(candidate) = v.pointer("/candidates/0") else { return String::new() };
        if let Some(finish) = candidate.get("finishReason").filter(|f| !f.is_null()) {
            g.finish = Some(finish.clone());
        }
        let mut said = String::new();
        for part in candidate.pointer("/content/parts").and_then(Value::as_array).into_iter().flatten() {
            let text = part.get("text").and_then(Value::as_str);
            let thought = part.get("thought").and_then(Value::as_bool) == Some(true);
            if let Some(text) = text
                && !thought
            {
                said.push_str(text);
            }
            // A run of text parts is one part, as the answer sent whole has it, unless a part
            // carries a signature of its own, which goes back to the model as it came.
            let joined = match (text, g.parts.last_mut()) {
                (Some(text), Some(last)) if last.get("text").is_some() && part.get("thoughtSignature").is_none() => {
                    let alike = (last.get("thought").and_then(Value::as_bool) == Some(true)) == thought;
                    if alike {
                        let had = last.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
                        last["text"] = json!(had + text);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !joined {
                g.parts.push(part.clone());
            }
        }
        said
    }

    /// The answer, as the provider would have sent it whole, once the body has all been fed: an
    /// error the stream carried, as the provider's error object, where it carried one.
    pub fn finish(mut self) -> Value {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.line(line.trim_end_matches('\r'));
        }
        self.dispatch();
        if let Some(error) = self.error {
            return error;
        }
        match self.provider {
            Provider::Anthropic => {
                let mut message = self.anthropic.message;
                let content: Vec<Value> = (self.anthropic.blocks.into_iter())
                    .map(|b| match b.kind.as_str() {
                        "tool_use" => {
                            let input = serde_json::from_str::<Value>(&b.json).unwrap_or_else(|_| json!({}));
                            json!({ "type": "tool_use", "id": b.id, "name": b.name, "input": input })
                        }
                        "text" => json!({ "type": "text", "text": b.text }),
                        other => json!({ "type": other }),
                    })
                    .collect();
                message.insert("content".into(), Value::Array(content));
                Value::Object(message)
            }
            Provider::Openai => {
                let o = self.openai;
                let mut message = json!({ "role": "assistant", "content": o.content });
                if !o.calls.is_empty() {
                    let calls: Vec<Value> = (o.calls.into_iter())
                        .map(|(id, name, arguments)| {
                            json!({ "id": id, "type": "function", "function": { "name": name, "arguments": arguments } })
                        })
                        .collect();
                    message["tool_calls"] = Value::Array(calls);
                }
                let mut out = json!({
                    "choices": [{ "index": 0, "finish_reason": o.finish.unwrap_or(Value::Null), "message": message }],
                });
                if let Some(id) = o.id {
                    out["id"] = id;
                }
                if let Some(usage) = o.usage {
                    out["usage"] = usage;
                }
                out
            }
            Provider::Gemini => {
                let g = self.gemini;
                let mut candidate = json!({ "content": { "role": "model", "parts": g.parts } });
                if let Some(finish) = g.finish {
                    candidate["finishReason"] = finish;
                }
                let mut out = json!({ "candidates": [candidate] });
                if let Some(usage) = g.usage {
                    out["usageMetadata"] = usage;
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_fed_byte_by_byte_reads_as_one_fed_whole() {
        let body = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"content\":[]}}\r\n\r\n\
                    event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                    : a comment\n\
                    event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Héllo\"}}\n\n\
                    event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n";
        let mut one = Stream::new(Provider::Anthropic);
        let mut said = String::new();
        for c in body.chars() {
            said.push_str(&one.feed(&c.to_string()));
        }
        assert_eq!(said, "Héllo");
        assert_eq!(one.finish(), assembled(Provider::Anthropic, body));
        assert_eq!(assembled(Provider::Anthropic, body)["content"], json!([{ "type": "text", "text": "Héllo" }]));
        assert_eq!(assembled(Provider::Anthropic, body)["stop_reason"], "end_turn");
        assert!(is_sse(body) && !is_sse("{}") && is_sse("  data: x"));
    }

    #[test]
    fn a_stream_cut_before_its_last_blank_line_still_ends() {
        let body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"A\"}]}}]}\n\ndata: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"B\"}]},\"finishReason\":\"STOP\"}]}";
        let mut stream = Stream::new(Provider::Gemini);
        assert_eq!(stream.feed(body), "A");
        let answer = stream.finish();
        assert_eq!(answer["candidates"][0]["content"]["parts"], json!([{ "text": "AB" }]));
        assert_eq!(answer["candidates"][0]["finishReason"], "STOP");
    }
}
