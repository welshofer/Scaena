//! A generation driven step by step on recorded answers, as a client drives it: arc selection,
//! then the slides, each request made by the client and its response handed back.

use scaena_narrato::{Brief, Generation, NarratoError, Provider, Settings, Step};

fn answer(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/answers/{name}.json", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn brief() -> Brief {
    Brief {
        topic: "Pitch a Series A round for our carbon-negative cargo platform.".into(),
        audience: "Investors".into(),
        goal: "Persuade".into(),
        ..Brief::default()
    }
}

fn settings(provider: Provider) -> Settings {
    Settings { provider, model: "a-model".into(), base: None, browser: false }
}

#[test]
fn a_deck_is_generated_in_two_calls() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert_eq!(g.step(), Step::Arc);
    let ask = g.request("sk-test", &[]).unwrap();
    assert_eq!(ask.headers["x-api-key"], "sk-test");
    let text = ask.body.unwrap()["messages"][0]["content"][0]["text"].as_str().unwrap().to_string();
    assert!(text.contains("=== CATALOG ===") && text.contains("Topic: Pitch a Series A round"));

    assert_eq!(g.answer(200, "OK", &answer("anthropic-arc")).unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution");
    assert_eq!(g.fallback(), None);

    let ask = g.request("sk-test", &[]).unwrap();
    let body = ask.body.unwrap();
    assert_eq!(body["max_tokens"], 64_000);
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("**Narrative Framework**: Problem\u{2013}Solution"));
    assert!(prompt.contains("You MUST generate AT LEAST 8 slides"));
    assert!(prompt.contains("**Slot shapes (Scaena)**"));

    assert_eq!(g.answer(200, "OK", &answer("anthropic-slides")).unwrap(), Step::Done);
    assert!(g.request("sk-test", &[]).is_none());
    let narrative = g.into_narrative().unwrap();
    assert_eq!(narrative.arc, "Problem\u{2013}Solution");
    assert_eq!(narrative.slides.len(), 5);
    assert!(!narrative.truncated);
    assert_eq!(narrative.slides[3].chart_data.as_ref().unwrap().series[0].values, [100.0, 80.0, 59.0]);
    assert_eq!(narrative.brief.source_content, None);
}

#[test]
fn an_arc_that_does_not_come_back_is_the_heuristics() {
    let mut g = Generation::new(brief(), settings(Provider::Gemini)).unwrap();
    assert_eq!(g.answer(0, "network down", "").unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution");
    assert_eq!(g.fallback(), Some("the request did not complete: network down"));
    let mut g = Generation::new(brief(), settings(Provider::Openai)).unwrap();
    g.answer(529, "Overloaded", r#"{"error":{"message":"overloaded"}}"#).unwrap();
    assert_eq!(g.fallback(), Some("529 Overloaded: overloaded"));
}

#[test]
fn too_few_slides_are_a_fragment_refused() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Data Storytelling"));
    assert_eq!(g.arc().unwrap().selection, "Data Storytelling");
    let two = r#"{"content":[{"type":"text","text":"{\"executiveSummary\":\"x\",\"slides\":[{\"slideTitle\":\"A\"},{\"slideTitle\":\"B\"}]}"}],"stop_reason":"end_turn"}"#;
    assert_eq!(g.answer(200, "OK", two), Err(NarratoError::Incomplete { delivered: 2, minimum: 5 }));
    // The step stands, to be asked again.
    assert_eq!(g.step(), Step::Slides);
    assert!(
        matches!(g.answer(401, "Unauthorized", r#"{"error":{"message":"bad key"}}"#), Err(NarratoError::Provider(m)) if m == "401 Unauthorized: bad key")
    );
}

#[test]
fn a_brief_that_does_not_read_is_refused_before_any_call() {
    let err = Generation::new(Brief::default(), settings(Provider::Anthropic)).unwrap_err();
    assert_eq!(err.to_string(), "Add a prompt describing what the deck is about.");
}
