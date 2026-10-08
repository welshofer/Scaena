//! scaena-chat held to the browser's assistant (ADR-0022): for one conversation
//! (`tests/assistant/conversation.json`), each provider's request, and its answer read back, are
//! what the browser's adapters made and read when `web/exchanges.mjs` recorded them
//! (`tests/assistant/exchanges.json`); and so are the page's system prompt and the line that
//! says what the editor shows. The two implementations cannot drift apart unseen.

use scaena_chat::{Ask, Call, Chat, Client, Facts, Listed, Message, Next, Provider, Seeing, Stop, Tool, ToolResult};
use serde_json::{Value, json};

fn read(name: &str) -> Value {
    let path = format!("{}/../../tests/assistant/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// What a page's `fetch` would have read as the body: the text itself, or the JSON.
fn body(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

fn id(provider: Provider) -> String {
    serde_json::to_value(provider).unwrap().as_str().unwrap().to_string()
}

#[test]
fn each_adapter_asks_and_reads_as_the_browsers_does() {
    let conversation = read("conversation.json");
    let record = read("exchanges.json");
    let tools: Vec<Tool> = serde_json::from_value(conversation["tools"].clone()).unwrap();
    let messages: Vec<Message> = serde_json::from_value(conversation["messages"].clone()).unwrap();
    let key = conversation["key"].as_str().unwrap();
    for provider in Provider::ALL {
        let id = id(provider);
        let kept = &record[&id];
        let ask = Ask {
            model: conversation["model"].as_str().unwrap(),
            key,
            base: None,
            system: conversation["system"].as_str().unwrap(),
            tools: &tools,
            messages: &messages,
            // The record is a page's: Anthropic's opt-in header with it.
            browser: true,
        };
        let request = serde_json::to_value(provider.turn(&ask)).unwrap();
        assert_eq!(request, kept["turn"]["request"], "{id}: the turn's request");
        let turn = provider.read_turn(200, "", &body(&conversation["answers"][&id])).unwrap();
        assert_eq!(serde_json::to_value(&turn).unwrap(), kept["turn"]["read"], "{id}: the turn read");

        let models = serde_json::to_value(provider.models(key, None, true)).unwrap();
        assert_eq!(models, kept["models"]["request"], "{id}: the models' request");
        let listed = provider.read_models(200, "", &body(&conversation["models"][&id])).unwrap();
        assert_eq!(json!(listed), kept["models"]["read"], "{id}: the models read");

        let failure = &conversation["failures"][&id];
        let status = failure["status"].as_u64().unwrap() as u16;
        let said = provider.read_turn(status, "", &body(&failure["body"])).unwrap_err().to_string();
        assert_eq!(json!(said), kept["failure"]["message"], "{id}: the failure said");
    }
}

#[test]
fn the_pages_prompt_is_the_browsers() {
    let told = &read("conversation.json")["prompt"];
    let record = &read("exchanges.json")["prompt"];
    let resources: Vec<Listed> = serde_json::from_value(told["resources"].clone()).unwrap();
    let author_deck = told["authorDeck"].as_str().unwrap();
    for (i, deck) in told["decks"].as_array().unwrap().iter().enumerate() {
        let facts: Facts = serde_json::from_value(deck.clone()).unwrap();
        let skills: Vec<String> = serde_json::from_value(told["bundleSkills"][i].clone()).unwrap();
        let system = scaena_chat::system_of(Client::Page, &facts, &resources, &skills, author_deck);
        assert_eq!(json!(system), record["system"][i], "deck {i}'s system prompt");
    }
    for (i, seeing) in told["seeings"].as_array().unwrap().iter().enumerate() {
        let seeing: Option<Seeing> = serde_json::from_value(seeing.clone()).unwrap();
        let line = scaena_chat::seen(Client::Page, seeing.as_ref());
        assert_eq!(json!(line), record["seen"][i], "seeing {i}");
    }
}

#[test]
fn a_question_runs_its_calls_until_the_model_answers() {
    let tools = scaena_chat::tools(&["deck_lint", "deck_render"]).unwrap();
    let mut chat = Chat::new(Provider::Anthropic, "a-model", "Be brief.".into(), tools);
    chat.ask("Is it clean?");
    let first = chat.request("sk-test");
    assert_eq!(first.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(first.headers["x-api-key"], "sk-test");
    assert!(!first.headers.contains_key("anthropic-dangerous-direct-browser-access"), "the Mac is no browser");
    let names: Vec<&str> =
        first.body.as_ref().unwrap()["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["deck_lint", "deck_render", "resource_read"]);

    let calls = json!({
        "content": [
            { "type": "text", "text": "Linting." },
            { "type": "tool_use", "id": "toolu_1", "name": "deck_lint", "input": { "state": "cover" } },
        ],
        "stop_reason": "tool_use",
        "usage": { "input_tokens": 10, "output_tokens": 5 },
    });
    let Next::Calls { text, calls, .. } = chat.answer(200, "OK", &calls.to_string()).unwrap() else {
        panic!("calls were made");
    };
    assert_eq!(text, "Linting.");
    assert_eq!(calls, [Call { id: "toolu_1".into(), name: "deck_lint".into(), args: json!({ "state": "cover" }) }]);
    let result = ToolResult {
        id: "toolu_1".into(),
        name: "deck_lint".into(),
        json: r#"{"findings":[]}"#.into(),
        error: false,
        png: None,
    };
    assert!(chat.results(vec![result]), "a round taken, of 32");

    // The next request carries the call and its result, in Anthropic's form.
    let second = chat.request("sk-test");
    let messages = second.body.as_ref().unwrap()["messages"].as_array().unwrap().clone();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["content"][1]["type"], "tool_use");
    assert_eq!(messages[2]["content"][0]["tool_use_id"], "toolu_1");

    let done = json!({ "content": [{ "type": "text", "text": "Clean." }], "stop_reason": "end_turn" });
    let next = chat.answer(200, "OK", &done.to_string()).unwrap();
    assert_eq!(next, Next::Done { text: "Clean.".into(), stop: Stop::End, usage: None });
    assert_eq!(chat.conversation.len(), 4);

    // A refusal leaves the conversation as it was.
    let refused = chat.answer(429, "Too Many Requests", r#"{"error":{"message":"slow down"}}"#);
    assert_eq!(refused.unwrap_err().to_string(), "429 Too Many Requests: slow down");
    assert_eq!(chat.conversation.len(), 4);

    chat.rounds = 1;
    chat.ask("Again?");
    chat.answer(200, "OK", &calls_again().to_string()).unwrap();
    assert!(!chat.results(vec![]), "the question has taken its one round");
    chat.forget();
    assert!(chat.conversation.is_empty());
}

fn calls_again() -> Value {
    json!({
        "content": [{ "type": "tool_use", "id": "toolu_2", "name": "deck_render", "input": {} }],
        "stop_reason": "tool_use",
    })
}

#[test]
fn only_the_newest_frames_are_shown_again() {
    let frame = |id: &str| ToolResult {
        id: id.into(),
        name: "deck_render".into(),
        json: "{}".into(),
        error: false,
        png: Some("iVBOR".into()),
    };
    let conversation =
        vec![Message::Tool { results: vec![frame("a"), frame("b")] }, Message::Tool { results: vec![frame("c")] }];
    let sent = scaena_chat::recent(&conversation, 2);
    let Message::Tool { results } = &sent[0] else { unreachable!() };
    assert_eq!(results[0].png, None);
    assert!(results[0].json.ends_with("(The frame is no longer shown: render it again to see it.)"));
    assert_eq!(results[1].png.as_deref(), Some("iVBOR"));
    let Message::Tool { results } = &sent[1] else { unreachable!() };
    assert_eq!(results[0].png.as_deref(), Some("iVBOR"));
}
