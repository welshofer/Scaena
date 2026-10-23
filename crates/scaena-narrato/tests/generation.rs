//! A generation driven step by step on recorded answers, as a client drives it: arc selection,
//! then the slides, each request made by the client and its response handed back.

use scaena_narrato::{Brief, Generation, Icon, NarratoError, Provider, Settings, Step, TRIES};

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

    assert_eq!(g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution");
    assert_eq!(g.fallback(), None);

    let ask = g.request("sk-test", &[]).unwrap();
    let body = ask.body.unwrap();
    assert_eq!(body["max_tokens"], 64_000);
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("**Narrative Framework**: Problem\u{2013}Solution"));
    assert!(prompt.contains("You MUST generate AT LEAST 8 slides"));
    assert!(prompt.contains("**Slot shapes (Scaena)**"));

    assert_eq!(g.answer(200, "OK", &answer("anthropic-slides"), None).unwrap(), Step::Done);
    assert!(g.request("sk-test", &[]).is_none());
    let narrative = g.into_narrative().unwrap();
    assert_eq!(narrative.arc, "Problem\u{2013}Solution");
    assert_eq!(narrative.slides.len(), 5);
    assert!(!narrative.truncated);
    assert_eq!(narrative.slides[3].chart_data.as_ref().unwrap().series[0].values, [100.0, 80.0, 59.0]);
    assert_eq!(narrative.brief.source_content, None);
}

/// The words the slides name icons by are asked for once each (PLAN G.23), and the icons the
/// client finds for them are kept with the narrative, for the deck it makes.
#[test]
fn the_icons_the_slides_name_are_asked_once_and_kept() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert!(g.icon_words().is_empty(), "none before the narrative is written");
    g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap();
    let slides = r#"{ "executiveSummary": "A pitch.", "slides": [
        { "slideTitle": "Tidewater", "layoutSuggestion": "Title", "keyBullets": [] },
        { "slideTitle": "Why us", "layoutSuggestion": "Bullet List",
          "keyBullets": ["Fast: a day", "Safe: sealed", "Shared: one ledger"], "icons": ["Rocket", " shield", "rocket"] },
        { "slideTitle": "The route", "layoutSuggestion": "Bullet List", "keyBullets": ["Ports", "Ships", "Crews"],
          "icons": ["-", "—", "\ud83d\ude80"] },
        { "slideTitle": "The cost", "layoutSuggestion": "Bullet List", "keyBullets": ["Less", "Sooner"] },
        { "slideTitle": "Sail with us", "layoutSuggestion": "Closing", "keyBullets": [] }
    ] }"#;
    g.written(slides, false).unwrap();
    assert_eq!(g.icon_words(), ["rocket", "shield"], "a word with no letter names none");
    let rocket = Icon { name: "rocket".into(), path: "M0,0L1,1Z".into(), view: [0.0, 0.0, 256.0, 256.0] };
    g.give_icons([("rocket".to_string(), rocket.clone())].into());
    assert_eq!(g.narrative().unwrap().icons["rocket"], rocket);
    assert!(!g.narrative().unwrap().icons.contains_key("shield"), "a word the client found none for has none");
}

#[test]
fn an_arc_that_does_not_come_back_is_the_heuristics() {
    let mut g = Generation::new(brief(), settings(Provider::Gemini)).unwrap();
    // A refusal is for good: the heuristic's arc at once.
    assert_eq!(g.answer(400, "Bad Request", r#"{"error":{"message":"no such model"}}"#, None).unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution");
    assert_eq!(g.fallback(), Some("400 Bad Request: no such model"));
    // No answer is for now: asked again, up to six times, then the heuristic's, saying so.
    let mut g = Generation::new(brief(), settings(Provider::Openai)).unwrap();
    for n in 1..TRIES {
        assert_eq!(g.answer(0, "network down", "", None).unwrap(), Step::Arc);
        assert_eq!(g.tried(), n);
    }
    assert_eq!(g.answer(529, "Overloaded", r#"{"error":{"message":"overloaded"}}"#, None).unwrap(), Step::Slides);
    assert_eq!(g.fallback(), Some("529 Overloaded: overloaded (asked 6 times)"));
    assert_eq!((g.again(), g.tried()), (None, 0));
}

/// A provider that is busy, fails for now, or does not answer is asked the same step again
/// (narrative REL-1): the wait its answer asks for, else one that doubles from two seconds;
/// an answer that then reads goes on; six for now in a row are an error that says so.
#[test]
fn a_busy_provider_is_asked_again() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Data Storytelling"));
    let request = g.request("k", &[]).unwrap();
    assert_eq!(g.answer(529, "Overloaded", r#"{"error":{"message":"Overloaded"}}"#, None).unwrap(), Step::Slides);
    assert_eq!((g.again(), g.tried()), (Some(2.0), 1));
    assert_eq!(g.request("k", &[]).unwrap(), request, "the same request, asked again");
    assert_eq!(g.answer(503, "Service Unavailable", "", Some("7")).unwrap(), Step::Slides);
    assert_eq!(g.again(), Some(7.0), "the provider's own wait");
    assert_eq!(
        g.answer(429, "Too Many Requests", r#"{"error":{"message":"Please try again in 850ms."}}"#, None).unwrap(),
        Step::Slides
    );
    assert_eq!(g.again(), Some(0.85), "the wait its words ask for");
    assert_eq!(g.answer(0, "connection reset", "", None).unwrap(), Step::Slides);
    assert_eq!((g.again(), g.tried()), (Some(16.0), 4), "doubling from two seconds, by the asks so far");
    assert_eq!(g.answer(200, "OK", &answer("anthropic-slides"), None).unwrap(), Step::Done);
    assert_eq!((g.again(), g.tried()), (None, 0));
    assert_eq!(g.narrative().unwrap().slides.len(), 5);

    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Data Storytelling"));
    for n in 1..TRIES {
        assert_eq!(g.answer(502, "Bad Gateway", "", Some("600")).unwrap(), Step::Slides);
        assert_eq!(g.again(), Some(60.0), "a minute at most");
        assert_eq!(g.tried(), n);
    }
    let last = g.answer(502, "Bad Gateway", "", None);
    assert!(matches!(&last, Err(NarratoError::Provider(m)) if m == "502 Bad Gateway: (asked 6 times)"), "{last:?}");
    assert_eq!((g.again(), g.tried(), g.step()), (None, 0, Step::Slides), "the step stands, to be asked again");
    // A failure for good is not asked again.
    assert!(g.answer(401, "Unauthorized", r#"{"error":{"message":"bad key"}}"#, None).is_err());
    assert_eq!(g.again(), None);
}

/// An arc the model names as nobody in the catalog spells it is the heuristic's, and the
/// generation says so, as it does when the call fails (narrative STAB-5); one it spells loosely
/// is the catalog's, with nothing said.
#[test]
fn an_arc_the_model_misnames_is_the_heuristics_and_says_so() {
    let answered = |selection: &str| {
        let text = format!(r#"{{"selection":"{selection}","reasoning":"Because.","candidates":[]}}"#);
        serde_json::json!({ "content": [{ "type": "text", "text": text }], "stop_reason": "end_turn" }).to_string()
    };
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert_eq!(g.answer(200, "OK", &answered("Kishōtenketsu")).unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution", "the heuristic's choice for a pitch");
    assert_eq!(g.fallback(), Some("the model named `Kishōtenketsu`, not an arc of the catalog"));
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.answer(200, "OK", &answered("Hero's Journey for Change")).unwrap();
    assert_eq!(g.arc().unwrap().selection, "Hero\u{2019}s Journey for Change");
    assert_eq!(g.fallback(), None);
    // Chosen by name, too: `--arc problem-solution` is the catalog's arc.
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("problem-solution"));
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution");
}

#[test]
fn too_few_slides_are_a_fragment_refused() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Data Storytelling"));
    assert_eq!(g.arc().unwrap().selection, "Data Storytelling");
    let two = r#"{"content":[{"type":"text","text":"{\"executiveSummary\":\"x\",\"slides\":[{\"slideTitle\":\"A\"},{\"slideTitle\":\"B\"}]}"}],"stop_reason":"end_turn"}"#;
    assert_eq!(g.answer(200, "OK", two, None), Err(NarratoError::Incomplete { delivered: 2, minimum: 5 }));
    // The step stands, to be asked again.
    assert_eq!(g.step(), Step::Slides);
    assert!(
        matches!(g.answer(401, "Unauthorized", r#"{"error":{"message":"bad key"}}"#, None), Err(NarratoError::Provider(m)) if m == "401 Unauthorized: bad key")
    );
}

#[test]
fn a_brief_that_does_not_read_is_refused_before_any_call() {
    let err = Generation::new(Brief::default(), settings(Provider::Anthropic)).unwrap_err();
    assert_eq!(err.to_string(), "Add a prompt describing what the deck is about.");
}
