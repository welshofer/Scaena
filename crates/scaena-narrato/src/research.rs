//! Deep research (PLAN G.8, ADR-0026 §6), as Vzestup runs it: a report on the topic, written
//! for the deck's audience and goal, its sources listed, then a source like any other
//! (`Deep Research: topic`). With a Gemini key, Gemini's research agent through the
//! Interactions API, polled until it is done; with an Anthropic key, the user's own model
//! with web search, a paused turn resumed. The client makes each request, keeps each wait,
//! and hands each answer back; nothing here touches the network.

use crate::NarratoError;
use crate::providers::{Provider, Request, anthropic_headers, at, read, with_json};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The research agent Vzestup asks for.
pub const DEEP_RESEARCH_AGENT: &str = "deep-research-pro-preview-12-2025";
/// The schema of the Interactions API the answers are read by.
const API_REVISION: &str = "2026-05-20";
/// How long the client waits between polls.
pub const POLL_MS: u64 = 30_000;
/// The most polls: 30 minutes at one every 30 seconds.
const MOST_POLLS: u32 = 60;
/// The most times a paused turn is resumed.
const MOST_RESUMES: u32 = 5;
/// The most the report may take.
const REPORT_TOKENS: u32 = 16_000;
/// The most searches a report may make.
const MOST_SEARCHES: u32 = 20;
/// Web search with dynamic filtering, then the basic tool for a model that lacks it.
const SEARCH: &str = "web_search_20260209";
const SEARCH_BASIC: &str = "web_search_20250305";

/// What the research is for (Vzestup's `ResearchJobRequest`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asked {
    pub topic: String,
    pub audience: String,
    pub goal: String,
    pub length: String,
}

/// The prompt the research is asked with, Vzestup's `buildResearchPrompt` byte for byte.
pub fn prompt(a: &Asked) -> String {
    let goal = match a.goal.as_str() {
        "Inform" => {
            "- Focus on clarity, comprehensiveness, and accuracy\n- Emphasize educational value\n- Include definitions and explanations"
        }
        "Persuade" => {
            "- Find evidence that supports key arguments\n- Address likely counterarguments\n- Include compelling statistics and proof points\n- Locate testimonials or endorsements"
        }
        "Inspire" => {
            "- Discover emotional hooks and motivational stories\n- Find vision statements and success stories\n- Include aspirational examples and possibilities\n- Locate transformative narratives"
        }
        _ => "",
    };
    format!(
        "You are a research analyst preparing comprehensive background research for a professional presentation.

PRESENTATION CONTEXT:
- Topic: {topic}
- Target Audience: {audience}
- Presentation Goal: {goal_name}
- Presentation Length: {length}

YOUR TASK:
Conduct thorough research to prepare authoritative source material for creating an effective presentation. Cover:

## 1. Core Content Research
- Find the most important and recent facts, statistics, and data points
- Identify key concepts that must be explained to this audience
- Locate authoritative sources and recent publications (prefer the last 2 years)
- Include relevant industry reports, academic papers, or government data

## 2. Audience-Specific Insights
- What does the audience ({audience}) likely already know?
- What misconceptions might they have that should be addressed?
- What questions are they most likely to ask?
- What level of detail is appropriate?

## 3. Narrative Material
- Compelling case studies or real-world examples
- Stories that illustrate key points effectively
- Quotes from experts or thought leaders
- Before/after comparisons or success stories

## 4. Visual Reference Ideas
- Data visualizations that would be effective
- Diagrams, frameworks, or models commonly used
- Iconic images or visual metaphors associated with this topic

## 5. Goal Alignment ({goal_name})
{goal}

## 6. Current Context
- Latest developments or news in this area
- Trending discussions or debates
- Emerging trends to be aware of

OUTPUT FORMAT:
A structured research report in Markdown:
- Clear headings (## main sections, ### subsections)
- Citations with URLs for all factual claims
- Context for why each finding matters for this presentation
- Most impactful information first
- Direct quotes where they add value
- Date/recency of sources when relevant",
        topic = a.topic,
        audience = a.audience,
        goal_name = a.goal,
        length = a.length,
    )
}

/// A source the report cites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cited {
    pub url: String,
    pub title: String,
}

/// A report written: its Markdown, the sources it cites, and whether it was cut short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub text: String,
    pub sources: Vec<Cited>,
    pub cut: bool,
}

impl Report {
    /// The source's text (Vzestup's `researchReportText`): the report, then its sources,
    /// numbered.
    pub fn source_text(&self) -> String {
        let listed: Vec<String> = self
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {} \u{2014} {}", i + 1, s.title, s.url))
            .collect();
        if listed.is_empty() {
            self.text.clone()
        } else {
            format!("{}\n\n## Sources\n\n{}", self.text, listed.join("\n"))
        }
    }
}

/// The source's name (Vzestup's `researchSourceName`): `Deep Research: ` and the topic's
/// first 80 characters.
pub fn source_name(topic: &str) -> String {
    format!("Deep Research: {}", topic.trim().chars().take(80).collect::<String>())
}

/// What the research asks of the client next.
#[derive(Debug, Clone, PartialEq)]
pub enum Next {
    /// Make this request now.
    Send(Request),
    /// Wait `ms`, then make the request.
    Wait { ms: u64, then: Request },
    /// The report, written.
    Done(Report),
}

/// Research under way.
#[derive(Debug)]
pub struct Research {
    model: String,
    base: Option<String>,
    browser: bool,
    prompt: String,
    state: State,
    /// What the agent says of its progress, as it says it.
    pub progress: Option<String>,
}

#[derive(Debug)]
enum State {
    Creating,
    Polling { id: String, polls: u32 },
    Searching { tool: &'static str, written: Vec<Value>, resumes: u32 },
    Done,
}

impl Research {
    /// Research for `asked`, with `provider`'s key and, for Anthropic, `model`. OpenAI runs
    /// none.
    pub fn new(
        asked: &Asked,
        provider: Provider,
        model: &str,
        base: Option<&str>,
        browser: bool,
    ) -> Result<Research, NarratoError> {
        if provider == Provider::Openai {
            return Err(NarratoError::Provider(
                "deep research takes a Gemini key (its research agent) or an Anthropic key (web search)".into(),
            ));
        }
        if provider == Provider::Anthropic && model.trim().is_empty() {
            return Err(NarratoError::Provider("deep research with an Anthropic key names its model".into()));
        }
        let state = if provider == Provider::Gemini {
            State::Creating
        } else {
            State::Searching { tool: SEARCH, written: Vec::new(), resumes: 0 }
        };
        Ok(Research {
            model: model.to_string(),
            base: base.filter(|b| !b.is_empty()).map(str::to_string),
            browser,
            prompt: prompt(asked),
            state,
            progress: None,
        })
    }

    /// The first request, `key` in its headers.
    pub fn start(&self, key: &str) -> Request {
        match &self.state {
            State::Creating => self.create(key),
            State::Searching { tool, written, .. } => self.search(key, tool, written),
            State::Polling { id, .. } => self.poll(key, id),
            State::Done => self.create(key),
        }
    }

    fn gemini_headers(&self, key: &str) -> BTreeMap<String, String> {
        with_json(BTreeMap::from([
            ("x-goog-api-key".to_string(), key.to_string()),
            ("api-revision".to_string(), API_REVISION.to_string()),
        ]))
    }

    fn create(&self, key: &str) -> Request {
        Request {
            method: "POST".into(),
            url: at(self.base.as_deref(), Provider::Gemini.base(), "/v1beta/interactions"),
            headers: self.gemini_headers(key),
            body: Some(
                json!({ "agent": DEEP_RESEARCH_AGENT, "input": self.prompt, "background": true, "store": true }),
            ),
        }
    }

    fn poll(&self, key: &str, id: &str) -> Request {
        let mut headers = self.gemini_headers(key);
        headers.remove("content-type");
        Request {
            method: "GET".into(),
            url: at(self.base.as_deref(), Provider::Gemini.base(), &format!("/v1beta/interactions/{id}")),
            headers,
            body: None,
        }
    }

    fn search(&self, key: &str, tool: &str, written: &[Value]) -> Request {
        let mut messages = vec![json!({ "role": "user", "content": self.prompt })];
        if !written.is_empty() {
            messages.push(json!({ "role": "assistant", "content": written }));
        }
        Request {
            method: "POST".into(),
            url: at(self.base.as_deref(), Provider::Anthropic.base(), "/v1/messages"),
            headers: with_json(anthropic_headers(key, self.browser)),
            body: Some(json!({
                "model": self.model,
                "max_tokens": REPORT_TOKENS,
                "tools": [{ "type": tool, "name": "web_search", "max_uses": MOST_SEARCHES }],
                "messages": messages,
            })),
        }
    }

    /// The answer to the last request: its status, its reason, and its body. What to do next.
    pub fn answer(&mut self, key: &str, status: u16, status_text: &str, body: &str) -> Result<Next, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        match std::mem::replace(&mut self.state, State::Done) {
            State::Creating => {
                let made = read(status, status_text, body).map_err(NarratoError::Provider)?;
                let id = (made["name"].as_str().filter(|n| !n.is_empty()))
                    .or_else(|| made["id"].as_str())
                    .map(|id| id.trim_start_matches("interactions/").to_string())
                    .ok_or_else(|| NarratoError::Provider("the research agent named no interaction".into()))?;
                let first = self.poll(key, &id);
                self.state = State::Polling { id, polls: 0 };
                Ok(Next::Send(first))
            }
            State::Polling { id, polls } => {
                let it = read(status, status_text, body).map_err(NarratoError::Provider)?;
                if let Some(progress) = it["progress"].as_str().filter(|p| !p.is_empty()) {
                    self.progress = Some(progress.to_string());
                }
                match it["status"].as_str().unwrap_or_default() {
                    "completed" => Ok(Next::Done(interaction(&it)?)),
                    "failed" => Err(NarratoError::Provider(
                        it["error"]["message"].as_str().unwrap_or("Deep Research interaction failed.").to_string(),
                    )),
                    "cancelled" => Err(NarratoError::Provider("Research was cancelled.".into())),
                    _ if polls + 1 >= MOST_POLLS => {
                        Err(NarratoError::Provider("Research timed out after 30 minutes.".into()))
                    }
                    _ => {
                        let then = self.poll(key, &id);
                        self.state = State::Polling { id, polls: polls + 1 };
                        Ok(Next::Wait { ms: POLL_MS, then })
                    }
                }
            }
            State::Searching { tool, mut written, resumes } => {
                // A model without web search's filtering is asked again with the basic tool.
                if status == 400 && tool == SEARCH && body.contains(SEARCH) {
                    self.state = State::Searching { tool: SEARCH_BASIC, written, resumes };
                    return Ok(Next::Send(self.start(key)));
                }
                let message = read(status, status_text, body).map_err(NarratoError::Provider)?;
                written.extend(message["content"].as_array().cloned().unwrap_or_default());
                match message["stop_reason"].as_str().unwrap_or_default() {
                    "pause_turn" if resumes < MOST_RESUMES => {
                        let next = self.search(key, tool, &written);
                        self.state = State::Searching { tool, written, resumes: resumes + 1 };
                        Ok(Next::Send(next))
                    }
                    "refusal" => Err(NarratoError::Provider(format!(
                        "the model declined to research this: {}",
                        message["stop_details"]["explanation"].as_str().unwrap_or("no reason given")
                    ))),
                    stop => {
                        let report = searched(&written, stop == "max_tokens" || stop == "pause_turn");
                        if report.text.trim().is_empty() {
                            return Err(NarratoError::Provider("the research came back with no report".into()));
                        }
                        Ok(Next::Done(report))
                    }
                }
            }
            State::Done => Err(NarratoError::Provider("the research is done already".into())),
        }
    }
}

/// The report a completed interaction holds (Vzestup's `extractResearchReport`): the text of its
/// last `model_output` step, the sources its annotations cite; the legacy `outputs` and
/// `response` where no step holds it; and, where nothing is cited, the report's own links.
fn interaction(it: &Value) -> Result<Report, NarratoError> {
    let mut sources: Vec<Cited> = Vec::new();
    let mut cite = |url: Option<&str>, title: Option<&str>| {
        if let (Some(url), Some(title)) = (url, title)
            && !sources.iter().any(|s| s.url == url)
        {
            sources.push(Cited { url: url.into(), title: title.into() });
        }
    };
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    let mut text = String::new();
    let steps = list(&it["steps"]);
    for step in &steps {
        for item in list(&step["content"]) {
            for a in list(&item["annotations"]) {
                cite(
                    a["uri"].as_str().or(a["web"]["uri"].as_str()),
                    a["title"].as_str().or(a["web"]["title"].as_str()),
                );
            }
        }
    }
    if let Some(last) = steps.iter().rev().find(|s| s["type"] == "model_output") {
        let texts: Vec<String> = list(&last["content"])
            .iter()
            .filter(|c| c["type"] == "text")
            .filter_map(|c| c["text"].as_str().map(str::to_string))
            .collect();
        text = texts.join("\n");
    }
    let mut harvest = |md: &Value| {
        for chunk in list(&md["groundingChunks"]) {
            cite(chunk["web"]["uri"].as_str(), chunk["web"]["title"].as_str());
        }
    };
    harvest(&it["groundingMetadata"]);
    for output in list(&it["outputs"]) {
        harvest(&output["groundingMetadata"]);
        if text.is_empty() {
            text = output["text"].as_str().unwrap_or_default().to_string();
        }
        for part in list(&output["parts"]) {
            if text.is_empty() {
                text = part["text"].as_str().unwrap_or_default().to_string();
            }
        }
    }
    if text.is_empty()
        && let Some(t) = it["response"]["text"].as_str()
    {
        text = t.to_string();
        harvest(&it["response"]["groundingMetadata"]);
    }
    if text.is_empty() {
        return Err(NarratoError::Provider("No research report found in interaction steps or outputs.".into()));
    }
    if sources.is_empty() {
        sources = links(&text);
    }
    Ok(Report { text, sources, cut: false })
}

/// The report a searched answer holds: its text blocks in order, each a run of the report, and
/// the sources its citations name, then the results its searches found.
fn searched(written: &[Value], cut: bool) -> Report {
    let mut text = String::new();
    let mut sources: Vec<Cited> = Vec::new();
    let mut cite = |url: Option<&str>, title: Option<&str>| {
        if let Some(url) = url
            && !sources.iter().any(|s| s.url == url)
        {
            sources.push(Cited { url: url.into(), title: title.unwrap_or(url).into() });
        }
    };
    for block in written {
        if block["type"] == "text" {
            text.push_str(block["text"].as_str().unwrap_or_default());
            for c in block["citations"].as_array().into_iter().flatten() {
                cite(c["url"].as_str(), c["title"].as_str());
            }
        }
    }
    for block in written.iter().filter(|b| b["type"] == "web_search_tool_result") {
        for r in block["content"].as_array().into_iter().flatten() {
            cite(r["url"].as_str(), r["title"].as_str());
        }
    }
    let text = text.trim().to_string();
    if sources.is_empty() {
        sources = links(&text);
    }
    Report { text, sources, cut }
}

/// The links a report writes: Markdown links first, then bare addresses, each titled by its
/// host.
fn links(text: &str) -> Vec<Cited> {
    let mut out: Vec<Cited> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else { break };
        let title = &after[..close];
        let target = &after[close + 2..];
        let end = target.find([')', ' ', '\n']).unwrap_or(target.len());
        let url = &target[..end];
        let web = url.starts_with("http://") || url.starts_with("https://");
        if web && !title.contains('[') && !out.iter().any(|c| c.url == url) {
            out.push(Cited { url: url.into(), title: title.into() });
        }
        rest = &target[end..];
    }
    for word in text.split(|c: char| c.is_whitespace() || "<>()[]".contains(c)) {
        if word.starts_with("http://") || word.starts_with("https://") {
            let url = word.trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let host = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url);
            if !out.iter().any(|c| c.url == url) {
                out.push(Cited { url: url.into(), title: host.into() });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked() -> Asked {
        Asked {
            topic: "Carbon-negative freight".into(),
            audience: "Investors".into(),
            goal: "Persuade".into(),
            length: "5-10 slides".into(),
        }
    }

    #[test]
    fn the_prompt_is_vzestups() {
        let p = prompt(&asked());
        assert!(p.starts_with("You are a research analyst preparing comprehensive background research"));
        assert!(p.contains("- Topic: Carbon-negative freight\n- Target Audience: Investors\n"));
        assert!(p.contains("## 5. Goal Alignment (Persuade)\n- Find evidence that supports key arguments\n"));
        assert!(p.contains("- What does the audience (Investors) likely already know?"));
        let sell = prompt(&Asked { goal: "Sell".into(), ..asked() });
        assert!(sell.contains("## 5. Goal Alignment (Sell)\n\n\n## 6. Current Context"));
        assert_eq!(source_name("  Freight  "), "Deep Research: Freight");
    }

    #[test]
    fn the_prompt_is_vzestups_byte_for_byte_for_each_goal() {
        // Rendered by Vzestup's own `buildResearchPrompt` (functions/src/research/parsing.ts).
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/answers/research-prompts.json");
        let theirs: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(theirs.len(), 6);
        for (goal, rendered) in theirs {
            assert_eq!(prompt(&Asked { goal: goal.clone(), ..asked() }), rendered, "{goal}");
        }
    }

    #[test]
    fn gemini_research_is_created_then_polled_until_it_is_done() {
        let mut r = Research::new(&asked(), Provider::Gemini, "", None, false).unwrap();
        let first = r.start("g-key");
        assert_eq!(first.url, "https://generativelanguage.googleapis.com/v1beta/interactions");
        assert_eq!(first.headers["x-goog-api-key"], "g-key");
        assert_eq!(first.body.as_ref().unwrap()["agent"], DEEP_RESEARCH_AGENT);
        let Next::Send(poll) = r.answer("g-key", 200, "OK", r#"{"id":"abc"}"#).unwrap() else { panic!() };
        assert_eq!(
            (poll.method.as_str(), poll.url.as_str()),
            ("GET", "https://generativelanguage.googleapis.com/v1beta/interactions/abc")
        );
        let busy = r#"{"status":"in_progress","progress":"Reading 12 sources"}"#;
        let Next::Wait { ms, then } = r.answer("g-key", 200, "OK", busy).unwrap() else { panic!() };
        assert_eq!((ms, then.url.as_str()), (POLL_MS, poll.url.as_str()));
        assert_eq!(r.progress.as_deref(), Some("Reading 12 sources"));
        let done = json!({ "status": "completed", "steps": [
            { "type": "thought", "content": [{ "type": "text", "text": "planning" }] },
            { "type": "model_output", "content": [
                { "type": "text", "text": "# Freight\nShipping emits 3%.", "annotations": [{ "uri": "https://imo.org/x", "title": "IMO" }] }
            ] }
        ] })
        .to_string();
        let Next::Done(report) = r.answer("g-key", 200, "OK", &done).unwrap() else { panic!() };
        assert_eq!(report.text, "# Freight\nShipping emits 3%.");
        assert_eq!(
            report.source_text(),
            "# Freight\nShipping emits 3%.\n\n## Sources\n\n1. IMO \u{2014} https://imo.org/x"
        );
        let mut failed = Research::new(&asked(), Provider::Gemini, "", None, false).unwrap();
        failed.answer("k", 200, "OK", r#"{"name":"interactions/xyz"}"#).unwrap();
        let e = failed.answer("k", 200, "OK", r#"{"status":"failed","error":{"message":"quota"}}"#).unwrap_err();
        assert!(e.to_string().contains("quota"));
    }

    #[test]
    fn anthropic_research_searches_resumes_a_paused_turn_and_lists_what_it_cites() {
        assert!(Research::new(&asked(), Provider::Openai, "m", None, false).is_err());
        let mut r = Research::new(&asked(), Provider::Anthropic, "a-model", None, false).unwrap();
        let first = r.start("sk");
        let body = first.body.unwrap();
        assert_eq!(body["tools"][0]["type"], SEARCH);
        assert_eq!(body["model"], "a-model");
        // A model without dynamic filtering is asked again with the basic tool.
        let refused = r#"{"type":"error","error":{"type":"invalid_request_error","message":"tool type 'web_search_20260209' is not supported"}}"#;
        let Next::Send(again) = r.answer("sk", 400, "Bad Request", refused).unwrap() else { panic!() };
        assert_eq!(again.body.unwrap()["tools"][0]["type"], SEARCH_BASIC);
        let paused = json!({ "stop_reason": "pause_turn", "content": [
            { "type": "server_tool_use", "id": "s1", "name": "web_search", "input": { "query": "freight emissions" } },
            { "type": "web_search_tool_result", "tool_use_id": "s1", "content": [
                { "type": "web_search_result", "url": "https://imo.org/x", "title": "IMO" }
            ] },
            { "type": "text", "text": "Shipping emits ", "citations": null },
            { "type": "text", "text": "about 3% of CO2.", "citations": [
                { "type": "web_search_result_location", "url": "https://iea.org/y", "title": "IEA", "cited_text": "3%" }
            ] }
        ] })
        .to_string();
        let Next::Send(resume) = r.answer("sk", 200, "OK", &paused).unwrap() else { panic!() };
        let messages = resume.body.unwrap()["messages"].clone();
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"].as_array().unwrap().len(), 4);
        let rest = json!({ "stop_reason": "end_turn", "content": [{ "type": "text", "text": " Rates are rising." }] })
            .to_string();
        let Next::Done(report) = r.answer("sk", 200, "OK", &rest).unwrap() else { panic!() };
        assert_eq!(report.text, "Shipping emits about 3% of CO2. Rates are rising.");
        let urls: Vec<&str> = report.sources.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(urls, ["https://iea.org/y", "https://imo.org/x"], "what is cited first");
        assert!(!report.cut);
    }

    #[test]
    fn a_report_with_no_citations_lists_its_own_links() {
        let found = links("See [the IEA](https://iea.org/y) and https://imo.org/x/report.html.");
        assert_eq!(
            found,
            [
                Cited { url: "https://iea.org/y".into(), title: "the IEA".into() },
                Cited { url: "https://imo.org/x/report.html".into(), title: "imo.org".into() },
            ]
        );
    }
}
