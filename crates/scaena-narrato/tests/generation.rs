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

/// The outline before the slides (narrative FUNC-1): asked once the arc is chosen, Scaena's own
/// call; settled by the person, it goes into the slides' prompt as Scaena's block, Narrato's text
/// standing as it is, and the narrative keeps it for the deck's sections.
#[test]
fn an_outline_is_asked_settled_and_written_to() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert!(g.outline_first().is_err(), "the arc is chosen first");
    g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap();
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
    assert_eq!(g.answer(200, "OK", &anthropic.to_string(), None).unwrap(), Step::Slides);
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

    g.answer(200, "OK", &answer("anthropic-slides"), None).unwrap();
    let narrative = g.into_narrative().unwrap();
    assert_eq!(narrative.outline.as_ref().map(|o| o.beats.len()), Some(6));
    assert!(serde_json::to_string(&narrative).unwrap().contains("\"bigIdea\":\"Cargo can pay for its carbon.\""));

    // A client that wants the slides at once asks for no outline, and its prompt has no block.
    let mut plain = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    plain.answer(200, "OK", &answer("anthropic-arc"), None).unwrap();
    let ask = plain.request("sk-test", &[]).unwrap();
    assert!(!ask.body.unwrap()["messages"][0]["content"][0]["text"].as_str().unwrap().contains("**Outline (Scaena)**"));
    // One settled with no claim is refused, and the step stands.
    let blank = scaena_narrato::Outline { big_idea: String::new(), beats: vec![] };
    assert!(matches!(plain.settle(&blank), Err(NarratoError::Answer(_))));
    assert_eq!(plain.step(), Step::Slides);
}

/// An outline that comes back but does not read is asked once more, at once; the second stands
/// as `unread`, the arc paid for kept, and the person writes the slides without one (lift-up
/// REL-3).
#[test]
fn an_unreadable_outline_is_asked_again_then_passed_over() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert!(g.skip_outline().is_err(), "the arc is chosen first");
    g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap();
    g.outline_first().unwrap();
    let first = g.request("sk-test", &[]).unwrap();
    // Prose around no JSON: it came back, and does not read.
    let prose = serde_json::json!({ "content": [{ "type": "text", "text": "Here is a lovely outline for you." }], "stop_reason": "end_turn" });
    assert_eq!(g.answer(200, "OK", &prose.to_string(), None).unwrap(), Step::Outline);
    assert_eq!((g.unread(), g.again(), g.outline()), (None, None, None), "asked once more, at once");
    assert_eq!(g.request("sk-test", &[]).unwrap().body, first.body, "the same request");
    // No claims: it reads as JSON, and is no outline.
    let empty = serde_json::json!({ "content": [{ "type": "text", "text": r#"{"bigIdea": "", "beats": []}"# }], "stop_reason": "end_turn" });
    assert_eq!(g.answer(200, "OK", &empty.to_string(), None).unwrap(), Step::Outline);
    let why = g.unread().expect("twice unreadable, it is said").to_string();
    assert!(why.ends_with("(asked 2 times)"), "{why}");
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution", "the arc paid for stands");

    // Passed over: the slides are written at once, with no outline block, and the deck is made.
    g.skip_outline().unwrap();
    assert_eq!((g.step(), g.unread(), g.outline()), (Step::Slides, None, None));
    let ask = g.request("sk-test", &[]).unwrap();
    let prompt = ask.body.unwrap()["messages"][0]["content"][0]["text"].as_str().unwrap().to_string();
    assert!(
        prompt.contains("**Narrative Framework**: Problem\u{2013}Solution") && !prompt.contains("**Outline (Scaena)**")
    );
    assert_eq!(g.answer(200, "OK", &answer("anthropic-slides"), None).unwrap(), Step::Done);
    assert_eq!(g.into_narrative().unwrap().slides.len(), 5);

    // An outline asked again after one unreadable answer starts its count afresh.
    let mut again = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    again.answer(200, "OK", &answer("anthropic-arc"), None).unwrap();
    again.outline_first().unwrap();
    again.answer(200, "OK", &prose.to_string(), None).unwrap();
    again.outline_first().unwrap();
    again.answer(200, "OK", &prose.to_string(), None).unwrap();
    assert_eq!(again.unread(), None, "one unreadable answer since it was asked afresh");
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
    assert_eq!(g.answer(200, "OK", &answered("Kishōtenketsu"), None).unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Problem\u{2013}Solution", "the heuristic's choice for a pitch");
    assert_eq!(g.fallback(), Some("the model named `Kishōtenketsu`, not an arc of the catalog"));
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.answer(200, "OK", &answered("Hero's Journey for Change"), None).unwrap();
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
