//! One structured call to the user's own model (ADR-0026): Anthropic's Messages API, OpenAI's
//! Chat Completions, or Gemini's generateContent, each asked for JSON in the shape a schema
//! gives, and its answer read back to the JSON's text. The requests go out as `scaena-chat`'s
//! do, with the key in their headers and nowhere else; no model is named here.

use crate::schema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How long to wait before a request answered for now is made again, the `tried`th time (from
/// 1): what its `Retry-After` says, in seconds; else what a rate limit's words ask for ("Please
/// try again in 12s"); else a wait that doubles from two seconds; a minute at most.
pub(crate) fn wait(after: Option<&str>, body: &str, tried: usize) -> f64 {
    let said = (after.and_then(|a| a.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s >= 0.0))
        .or_else(|| try_again_in(&body.to_lowercase()));
    said.unwrap_or((2u32 << tried.saturating_sub(1).min(16)) as f64).min(crate::LONGEST_WAIT)
}

/// The wait a rate limit's words ask for: "Please try again in 12s", "in 850ms".
pub(crate) fn try_again_in(text: &str) -> Option<f64> {
    let rest = &text[text.find("try again in ")? + "try again in ".len()..];
    let number: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let value: f64 = number.parse().ok()?;
    let unit = &rest[number.len()..];
    Some(if unit.starts_with("ms") {
        value / 1000.0
    } else if unit.starts_with('m') && !unit.starts_with("ms") {
        value * 60.0
    } else {
        value
    })
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

    /// The most bytes of PDFs one request may carry, before base64 makes them a third larger:
    /// Gemini takes 20 MB a request inline, Anthropic and OpenAI 32 MB, room left for the prompt.
    pub fn document_bytes(self) -> usize {
        match self {
            Provider::Gemini => 14 << 20,
            Provider::Anthropic | Provider::Openai => 22 << 20,
        }
    }

    /// The provider `name` names, as the settings write it (`anthropic`, `openai`, `gemini`).
    pub fn named(name: &str) -> Option<Provider> {
        Provider::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(name))
    }

    /// The request that lists the models `key` can use.
    pub fn models(self, key: &str, base: Option<&str>, browser: bool) -> Request {
        let (path, headers) = match self {
            Provider::Anthropic => ("/v1/models?limit=100", anthropic_headers(key, browser)),
            Provider::Openai => ("/v1/models", BTreeMap::from([("authorization".into(), format!("Bearer {key}"))])),
            Provider::Gemini => {
                ("/v1beta/models?pageSize=1000", BTreeMap::from([("x-goog-api-key".into(), key.into())]))
            }
        };
        Request { method: "GET".into(), url: at(base, self.base(), path), headers, body: None }
    }

    /// The models an answer to [`Provider::models`] lists, as the provider lists them (OpenAI's
    /// sorted, since it gives no order).
    pub fn read_models(self, status: u16, status_text: &str, body: &str) -> Result<Vec<String>, String> {
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
}

/// An HTTP request for the client to make.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    /// JSON; none for a GET.
    pub body: Option<Value>,
}

/// Which contract an answer keeps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Arc,
    Slides,
    /// Texts that run past their place, rewritten to fit (Scaena's own call, [`crate::fit`]).
    Fit,
    /// The deck seen as it is drawn, and what to change on it (Scaena's own call, [`crate::look`]).
    Look,
}

/// A document the model reads beside the prompt: a PDF, sent as itself (ADR-0026).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Its name, as the user knows it.
    pub name: String,
    /// `application/pdf`.
    pub media_type: String,
    /// Its bytes, in base64.
    pub data: String,
}

/// A picture the model looks at beside the prompt, after the words that say what it is: a slide
/// as it is drawn (PLAN G.13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    /// What is said of it, before it.
    pub caption: String,
    /// `image/jpeg` or `image/png`.
    pub media_type: String,
    /// Its bytes, in base64.
    pub data: String,
}

/// One structured call.
#[derive(Debug, Clone, Copy)]
pub struct Call<'a> {
    pub model: &'a str,
    pub key: &'a str,
    /// The API's address; the provider's own where none is given.
    pub base: Option<&'a str>,
    /// Whether a browser asks: Anthropic takes a browser's call only with its opt-in header.
    pub browser: bool,
    /// The rider: Anthropic's system prompt, OpenAI's system message. Gemini, held by its
    /// schema, is sent none, as Vzestup sends it none.
    pub system: &'a str,
    pub prompt: &'a str,
    pub shape: Shape,
    /// Whether Anthropic is told the shape too, beside the rider: Vzestup tells it for the
    /// slides, not for arc selection.
    pub anthropic_schema: bool,
    pub max_tokens: u32,
    /// Gemini's temperature; the others keep their own.
    pub temperature: Option<f64>,
    pub documents: &'a [Document],
    /// Pictures after the documents, each after its caption, and before the prompt.
    pub pictures: &'a [Picture],
}

/// Why a model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stop {
    /// It was done.
    End,
    /// It ran out of room: the answer may be cut short.
    Length,
    Other,
}

/// Tokens in and out, as the provider counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

/// What a model answered: its text, why it stopped, and what it cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub text: String,
    pub stop: Stop,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl Provider {
    /// The request for `call`.
    pub fn structured(self, call: &Call) -> Request {
        let contract = match call.shape {
            Shape::Arc => schema::arc_gemini(),
            Shape::Slides => schema::slides_gemini(),
            Shape::Fit => crate::fit::schema_gemini(),
            Shape::Look => crate::look::schema_gemini(),
        };
        let name = match call.shape {
            Shape::Arc => "arc",
            Shape::Slides => "deck",
            Shape::Fit => "fit",
            Shape::Look => "look",
        };
        match self {
            Provider::Anthropic => {
                let mut content: Vec<Value> = (call.documents.iter())
                    .map(|d| {
                        json!({
                            "type": "document",
                            "source": { "type": "base64", "media_type": d.media_type, "data": d.data },
                            "title": d.name,
                        })
                    })
                    .collect();
                for p in call.pictures {
                    content.push(json!({ "type": "text", "text": p.caption }));
                    content.push(json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": p.media_type, "data": p.data },
                    }));
                }
                content.push(json!({ "type": "text", "text": call.prompt }));
                let mut body = json!({
                    "model": call.model,
                    "max_tokens": call.max_tokens,
                    "system": [{ "type": "text", "text": call.system, "cache_control": { "type": "ephemeral" } }],
                    "messages": [{ "role": "user", "content": content }],
                });
                if call.anthropic_schema {
                    body["output_config"] =
                        json!({ "format": { "type": "json_schema", "schema": schema::json_schema(&contract) } });
                }
                Request {
                    method: "POST".into(),
                    url: at(call.base, self.base(), "/v1/messages"),
                    headers: with_json(anthropic_headers(call.key, call.browser)),
                    body: Some(body),
                }
            }
            Provider::Openai => {
                let mut content: Vec<Value> = (call.documents.iter())
                    .map(|d| {
                        json!({
                            "type": "file",
                            "file": { "filename": d.name, "file_data": format!("data:{};base64,{}", d.media_type, d.data) },
                        })
                    })
                    .collect();
                for p in call.pictures {
                    content.push(json!({ "type": "text", "text": p.caption }));
                    content.push(json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:{};base64,{}", p.media_type, p.data), "detail": "high" },
                    }));
                }
                content.push(json!({ "type": "text", "text": call.prompt }));
                let body = json!({
                    "model": call.model,
                    "messages": [
                        { "role": "system", "content": call.system },
                        { "role": "user", "content": content },
                    ],
                    "response_format": {
                        "type": "json_schema",
                        "json_schema": { "name": name, "strict": true, "schema": schema::strict(&schema::json_schema(&contract)) },
                    },
                    "max_completion_tokens": call.max_tokens,
                });
                Request {
                    method: "POST".into(),
                    url: at(call.base, self.base(), "/v1/chat/completions"),
                    headers: with_json(BTreeMap::from([("authorization".into(), format!("Bearer {}", call.key))])),
                    body: Some(body),
                }
            }
            Provider::Gemini => {
                let mut parts: Vec<Value> = (call.documents.iter())
                    .map(|d| json!({ "inline_data": { "mime_type": d.media_type, "data": d.data } }))
                    .collect();
                for p in call.pictures {
                    parts.push(json!({ "text": p.caption }));
                    parts.push(json!({ "inline_data": { "mime_type": p.media_type, "data": p.data } }));
                }
                parts.push(json!({ "text": call.prompt }));
                let mut config = json!({
                    "responseMimeType": "application/json",
                    "responseSchema": contract,
                    "maxOutputTokens": call.max_tokens,
                });
                if let Some(t) = call.temperature {
                    config["temperature"] = json!(t);
                }
                let path = format!("/v1beta/models/{}:generateContent", component(call.model));
                Request {
                    method: "POST".into(),
                    url: at(call.base, self.base(), &path),
                    headers: with_json(BTreeMap::from([("x-goog-api-key".into(), call.key.into())])),
                    body: Some(json!({ "contents": [{ "role": "user", "parts": parts }], "generationConfig": config })),
                }
            }
        }
    }

    /// The answer to a [`Provider::structured`] request: its text, which should be the JSON
    /// asked for. A refusal, a block, or an error status is an error, in the provider's words.
    pub fn read_answer(self, status: u16, status_text: &str, body: &str) -> Result<Answer, String> {
        let body = read(status, status_text, body)?;
        let count = |v: &Value, key: &str| v.get(key).and_then(Value::as_u64).unwrap_or(0);
        match self {
            Provider::Anthropic => {
                let text: String = (body.get("content").and_then(Value::as_array).into_iter().flatten())
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect();
                let stop = match body.get("stop_reason").and_then(Value::as_str) {
                    Some("end_turn" | "stop_sequence") => Stop::End,
                    Some("max_tokens") => Stop::Length,
                    Some("refusal") => return Err(format!("{} declined to write it", self.name())),
                    _ => Stop::Other,
                };
                let usage = body
                    .get("usage")
                    .map(|u| Usage { input: count(u, "input_tokens"), output: count(u, "output_tokens") });
                Ok(Answer { text, stop, usage })
            }
            Provider::Openai => {
                let choice = body.pointer("/choices/0").ok_or("OpenAI answered with no choice")?;
                if let Some(refused) = choice.pointer("/message/refusal").and_then(Value::as_str) {
                    return Err(format!("OpenAI declined to write it: {refused}"));
                }
                let text = choice.pointer("/message/content").and_then(Value::as_str).unwrap_or_default().to_string();
                let stop = match choice.get("finish_reason").and_then(Value::as_str) {
                    Some("stop") => Stop::End,
                    Some("length") => Stop::Length,
                    Some("content_filter") => return Err("OpenAI's content filter stopped the answer".into()),
                    _ => Stop::Other,
                };
                let usage = body
                    .get("usage")
                    .map(|u| Usage { input: count(u, "prompt_tokens"), output: count(u, "completion_tokens") });
                Ok(Answer { text, stop, usage })
            }
            Provider::Gemini => {
                if let Some(reason) = body.pointer("/promptFeedback/blockReason").and_then(Value::as_str) {
                    return Err(format!("Gemini blocked the request ({reason})"));
                }
                let candidate = body.pointer("/candidates/0").ok_or("Gemini answered with no candidate")?;
                let reason = candidate.get("finishReason").and_then(Value::as_str);
                let stop = match reason {
                    Some("STOP") | None => Stop::End,
                    Some("MAX_TOKENS") => Stop::Length,
                    Some(blocked @ ("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII")) => {
                        return Err(format!("Gemini stopped the answer ({blocked})"));
                    }
                    Some(_) => Stop::Other,
                };
                let text: String =
                    (candidate.pointer("/content/parts").and_then(Value::as_array).into_iter().flatten())
                        .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                        .collect();
                let usage = body.get("usageMetadata").map(|u| Usage {
                    input: count(u, "promptTokenCount"),
                    output: count(u, "candidatesTokenCount") + count(u, "thoughtsTokenCount"),
                });
                Ok(Answer { text, stop, usage })
            }
        }
    }
}

pub(crate) fn anthropic_headers(key: &str, browser: bool) -> BTreeMap<String, String> {
    let mut out =
        BTreeMap::from([("x-api-key".to_string(), key.to_string()), ("anthropic-version".into(), "2023-06-01".into())]);
    if browser {
        // A browser calls the API with the user's own key, which never leaves the page but for
        // Anthropic (ADR-0006).
        out.insert("anthropic-dangerous-direct-browser-access".into(), "true".into());
    }
    out
}

pub(crate) fn with_json(mut headers: BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers.insert("content-type".into(), "application/json".into());
    headers
}

/// `body` as JSON, or the provider's error, said as the provider says it.
pub(crate) fn read(status: u16, status_text: &str, text: &str) -> Result<Value, String> {
    let body: Option<Value> = if text.is_empty() { Some(json!({})) } else { serde_json::from_str(text).ok() };
    if !(200..300).contains(&status) {
        let said = (body.as_ref())
            .and_then(|b| b.pointer("/error/message").or_else(|| b.get("message")))
            .and_then(Value::as_str)
            .map_or_else(|| crate::js::first_units(text, 300), str::to_string);
        return Err(format!("{status} {status_text}: {said}").trim().to_string());
    }
    body.ok_or_else(|| format!("not JSON: {}", crate::js::first_units(text, 300)))
}

pub(crate) fn at(base: Option<&str>, fallback: &str, path: &str) -> String {
    let base = base.filter(|b| !b.is_empty()).unwrap_or(fallback);
    format!("{}{path}", base.trim_end_matches('/'))
}

/// `text` as one part of a URL's path.
pub(crate) fn component(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
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

    fn call(shape: Shape, documents: &[Document]) -> Call<'_> {
        Call {
            model: "model-x",
            key: "k",
            base: None,
            browser: false,
            system: "rider",
            prompt: "prompt",
            shape,
            anthropic_schema: shape == Shape::Slides,
            max_tokens: 1000,
            temperature: Some(0.7),
            documents,
            pictures: &[],
        }
    }

    #[test]
    fn each_provider_is_asked_for_its_structured_answer() {
        let pdf = [Document { name: "q3.pdf".into(), media_type: "application/pdf".into(), data: "JVBE".into() }];
        let a = Provider::Anthropic.structured(&call(Shape::Slides, &pdf));
        assert_eq!(a.url, "https://api.anthropic.com/v1/messages");
        let body = a.body.unwrap();
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["messages"][0]["content"][0]["type"], "document");
        assert_eq!(body["messages"][0]["content"][1], json!({ "type": "text", "text": "prompt" }));
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        let arc = Provider::Anthropic.structured(&call(Shape::Arc, &[])).body.unwrap();
        assert!(arc.get("output_config").is_none(), "arc selection is held by its rider alone, as Vzestup's is");

        let o = Provider::Openai.structured(&call(Shape::Slides, &pdf)).body.unwrap();
        assert_eq!(o["response_format"]["json_schema"]["strict"], true);
        assert_eq!(o["messages"][0], json!({ "role": "system", "content": "rider" }));
        assert_eq!(o["messages"][1]["content"][0]["file"]["file_data"], "data:application/pdf;base64,JVBE");
        assert_eq!(o["max_completion_tokens"], 1000);

        let g = Provider::Gemini.structured(&Call { model: "models x", ..call(Shape::Slides, &pdf) });
        assert_eq!(g.url, "https://generativelanguage.googleapis.com/v1beta/models/models%20x:generateContent");
        let body = g.body.unwrap();
        assert_eq!(body["generationConfig"]["responseSchema"]["type"], "OBJECT");
        assert_eq!(body["generationConfig"]["temperature"], 0.7);
        assert_eq!(body["contents"][0]["parts"][0]["inline_data"]["mime_type"], "application/pdf");
        assert!(body.get("systemInstruction").is_none());
    }

    #[test]
    fn a_slide_drawn_goes_to_each_provider_as_its_picture_after_what_is_said_of_it() {
        let drawn = [Picture { caption: "Slide 1 of 2".into(), media_type: "image/jpeg".into(), data: "/9j/".into() }];
        let look = Call { pictures: &drawn, ..call(Shape::Look, &[]) };
        let a = Provider::Anthropic.structured(&look).body.unwrap();
        let content = &a["messages"][0]["content"];
        assert_eq!(content[0], json!({ "type": "text", "text": "Slide 1 of 2" }));
        assert_eq!(content[1]["source"], json!({ "type": "base64", "media_type": "image/jpeg", "data": "/9j/" }));
        assert_eq!(content[2], json!({ "type": "text", "text": "prompt" }));
        let o = Provider::Openai.structured(&look).body.unwrap();
        assert_eq!(o["messages"][1]["content"][1]["image_url"]["url"], "data:image/jpeg;base64,/9j/");
        assert_eq!(o["response_format"]["json_schema"]["name"], "look");
        let g = Provider::Gemini.structured(&look).body.unwrap();
        let parts = &g["contents"][0]["parts"];
        assert_eq!(parts[0], json!({ "text": "Slide 1 of 2" }));
        assert_eq!(parts[1]["inline_data"], json!({ "mime_type": "image/jpeg", "data": "/9j/" }));
        assert_eq!(g["generationConfig"]["responseSchema"]["properties"]["slides"]["type"], "ARRAY");
    }

    #[test]
    fn each_answer_reads_to_its_text() {
        let a = Provider::Anthropic
            .read_answer(
                200,
                "OK",
                r#"{"content":[{"type":"thinking","thinking":"…"},{"type":"text","text":"{\"a\":1}"}],"stop_reason":"max_tokens","usage":{"input_tokens":5,"output_tokens":7}}"#,
            )
            .unwrap();
        assert_eq!(
            (a.text.as_str(), a.stop, a.usage),
            ("{\"a\":1}", Stop::Length, Some(Usage { input: 5, output: 7 }))
        );
        let o = Provider::Openai
            .read_answer(
                200,
                "OK",
                r#"{"choices":[{"message":{"content":"{}","refusal":null},"finish_reason":"stop"}]}"#,
            )
            .unwrap();
        assert_eq!((o.text.as_str(), o.stop), ("{}", Stop::End));
        let g = Provider::Gemini
            .read_answer(
                200,
                "OK",
                r#"{"candidates":[{"content":{"parts":[{"text":"plan","thought":true},{"text":"{}"}]},"finishReason":"STOP"}]}"#,
            )
            .unwrap();
        assert_eq!(g.text, "{}");
        assert_eq!(
            Provider::Anthropic.read_answer(401, "Unauthorized", r#"{"error":{"message":"invalid x-api-key"}}"#),
            Err("401 Unauthorized: invalid x-api-key".into())
        );
        assert!(Provider::Gemini.read_answer(200, "OK", r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#).is_err());
    }
}
