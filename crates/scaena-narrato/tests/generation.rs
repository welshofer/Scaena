//! A generation driven step by step on recorded answers, as a client drives it: arc selection,
//! then the slides, each request made by the client and its response handed back.

use scaena_narrato::{Brief, Generation, Icon, NarratoError, Provider, Settings, Step};

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

/// The outline before the slides (narrative FUNC-1): asked once the arc is chosen, Scaena's own
/// call; settled by the person, it goes into the slides' prompt as Scaena's block, Narrato's text
/// standing as it is, and the narrative keeps it for the deck's sections.
#[test]
fn an_outline_is_asked_settled_and_written_to() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert!(g.outline_first().is_err(), "the arc is chosen first");
    g.answer(200, "OK", &answer("anthropic-arc")).unwrap();
    let planned = g.plan().unwrap();
    assert_eq!(planned.len(), 6, "eight slides over Problem\u{2013}Solution's six beats");
    assert!(planned.iter().all(|b| b.claim.is_empty()) && planned.iter().map(|b| b.slides).sum::<usize>() == 8);
    g.outline_first().unwrap();
    assert_eq!(g.step(), Step::Outline);
    let ask = g.request("sk-test", &[]).unwrap();
    let body = ask.body.unwrap();
    assert_eq!(body["system"][0]["text"].as_str().unwrap(), scaena_narrato::outline::OUTLINE_SYSTEM);
    assert_eq!(body["output_config"]["format"]["schema"]["properties"]["bigIdea"]["type"], "string");
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("It follows the Problem\u{2013}Solution arc"), "{prompt}");
    assert!(prompt.contains("Beat 1 \"Problem: Define core challenge and stakes\" \u{2192} 1 slide"), "{prompt}");
    assert!(prompt.contains("Beat 6 \"Call to Action: Next steps\" \u{2192} 2 slides"), "{prompt}");

    let written = r#"{"bigIdea": "Cargo can pay for its carbon.", "beats": [
        {"beat": 1, "claim": "Shipping's carbon bill is coming due."},
        {"beat": 2, "claim": "Carriers cannot cut it with today's ships."},
        {"beat": 3, "claim": "Tidewater's lanes already run carbon-negative."},
        {"beat": 4, "claim": "Shippers pay more for them."},
        {"beat": 5, "claim": "Three lanes become thirty."},
        {"beat": 6, "claim": "Join the round."}
    ]}"#;
    let anthropic = serde_json::json!({ "content": [{ "type": "text", "text": written }], "stop_reason": "end_turn" });
    assert_eq!(g.answer(200, "OK", &anthropic.to_string()).unwrap(), Step::Slides);
    let outline = g.outline().unwrap().clone();
    assert_eq!(outline.big_idea, "Cargo can pay for its carbon.");
    assert_eq!(outline.beats[2].claim, "Tidewater's lanes already run carbon-negative.");
    assert_eq!(g.plan().unwrap(), outline.beats, "the plan is the outline's once one is written");

    // The person moves the proof before the mechanism, drops the second beat, and adds one.
    let mut settled = outline.clone();
    let proof = settled.beats.remove(3);
    settled.beats.insert(2, proof);
    settled.beats.remove(1);
    settled.beats.push(scaena_narrato::outline::Beat { name: "Thanks".into(), claim: "Thank you.".into(), slides: 0 });
    g.settle(&settled).unwrap();
    assert_eq!(g.step(), Step::Slides);
    let kept = g.outline().unwrap();
    let names: Vec<&str> = kept.beats.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names.len(), 6);
    assert_eq!(names[1], outline.beats[3].name, "the proof's beat moved up");
    assert_eq!(kept.beats[1].claim, "Shippers pay more for them.");
    assert_eq!(names[5], "Thanks");
    assert_eq!(kept.beats.iter().map(|b| b.slides).sum::<usize>(), 8, "the slides spread over the beats kept");

    let ask = g.request("sk-test", &[]).unwrap();
    let prompt = ask.body.unwrap()["messages"][0]["content"][0]["text"].as_str().unwrap().to_string();
    let narrato_end = prompt.find("**Slot shapes (Scaena)**").unwrap();
    let plain = scaena_narrato::prompt::generation(g.brief(), scaena_narrato::arcs::arc("Problem\u{2013}Solution"), 8);
    assert_eq!(&prompt[..narrato_end], &plain[..narrato_end], "Narrato's text stands as it is");
    assert!(prompt.contains("**Outline (Scaena)**"), "{prompt}");
    assert!(prompt.contains("The big idea, which every slide serves: \"Cargo can pay for its carbon.\""));
    assert!(
        prompt.contains(&format!("Beat 2 \"{}\" \u{2192} slide 2: Shippers pay more for them.", outline.beats[3].name)),
        "{prompt}"
    );
    assert!(prompt.contains("Beat 6 \"Thanks\" \u{2192} slide"), "{prompt}");

    g.answer(200, "OK", &answer("anthropic-slides")).unwrap();
    let narrative = g.into_narrative().unwrap();
    assert_eq!(narrative.outline.as_ref().map(|o| o.beats.len()), Some(6));
    assert!(serde_json::to_string(&narrative).unwrap().contains("\"bigIdea\":\"Cargo can pay for its carbon.\""));

    // A client that wants the slides at once asks for no outline, and its prompt has no block.
    let mut plain = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    plain.answer(200, "OK", &answer("anthropic-arc")).unwrap();
    let ask = plain.request("sk-test", &[]).unwrap();
    assert!(!ask.body.unwrap()["messages"][0]["content"][0]["text"].as_str().unwrap().contains("**Outline (Scaena)**"));
    // One settled with no claim is refused, and the step stands.
    let blank = scaena_narrato::Outline { big_idea: String::new(), beats: vec![] };
    assert!(matches!(plain.settle(&blank), Err(NarratoError::Answer(_))));
    assert_eq!(plain.step(), Step::Slides);
}

/// The words the slides name icons by are asked for once each (PLAN G.23), and the icons the
/// client finds for them are kept with the narrative, for the deck it makes.
#[test]
fn the_icons_the_slides_name_are_asked_once_and_kept() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert!(g.icon_words().is_empty(), "none before the narrative is written");
    g.answer(200, "OK", &answer("anthropic-arc")).unwrap();
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
