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

/// An arc answered in a sentence around its JSON keeps the model's arc, with no fall back to the
/// heuristic's (lift-up REL-3).
#[test]
fn an_arc_answered_in_a_sentence_is_the_models() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    let said = "Here is my pick: {\"selection\": \"Hero's Journey\", \"reasoning\": \"A founder's road.\", \"candidates\": []}";
    let anthropic = serde_json::json!({ "content": [{ "type": "text", "text": said }], "stop_reason": "end_turn" });
    assert_eq!(g.answer(200, "OK", &anthropic.to_string(), None).unwrap(), Step::Slides);
    assert_eq!(g.arc().unwrap().selection, "Hero's Journey");
    assert_eq!(g.fallback(), None, "the model's arc, not the heuristic's");
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

/// A deck grounded in a PDF has its outline written with the PDF beside the prompt, as its slides
/// are (lift-up FUNC-3); the arc sees none.
#[test]
fn the_outline_is_asked_with_the_documents_beside_it() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    let pdf = scaena_narrato::Document {
        name: "deck.pdf".into(),
        media_type: "application/pdf".into(),
        data: "JVBERi0xLjcK".into(),
    };
    let arc = g.request("sk-test", std::slice::from_ref(&pdf)).unwrap();
    assert!(!serde_json::to_string(&arc.body).unwrap().contains("\"document\""), "the arc sees no document");
    g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap();
    g.outline_first().unwrap();
    let outline = g.request("sk-test", std::slice::from_ref(&pdf)).unwrap().body.unwrap();
    let content = outline["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "document", "{content:?}");
    assert_eq!(content[0]["source"]["data"], "JVBERi0xLjcK");
    assert_eq!(outline["system"][0]["text"].as_str().unwrap(), scaena_narrato::outline::OUTLINE_SYSTEM);
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

/// Anthropic's answer of `text`, stopped where `cut` says it ran out of room.
fn anthropic(text: &str, cut: bool) -> String {
    serde_json::json!({
        "type": "message",
        "content": [{ "type": "text", "text": text }],
        "stop_reason": if cut { "max_tokens" } else { "end_turn" },
    })
    .to_string()
}

fn slides(from: usize, to: usize) -> Vec<serde_json::Value> {
    (from..=to).map(|n| serde_json::json!({ "slideTitle": format!("Slide {n}"), "beatName": "Problem" })).collect()
}

/// An answer cut short of the length asked (lift-up REL-2): the rest is asked for with the
/// slides' own prompt and those written so far after it, and the narrative is the two answers
/// as one, whole, which `--slides` reads again.
#[test]
fn slides_cut_short_ask_for_the_rest() {
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Problem\u{2013}Solution"));
    let first =
        serde_json::json!({ "executiveSummary": "Cargo, carbon-negative.", "slides": slides(1, 5) }).to_string();
    // Cut inside the sixth slide: five close.
    let cut = format!("{}, {{\"slideTitle\": \"Slide 6\", \"keyB", &first[..first.len() - 2]);
    assert_eq!(g.answer(200, "OK", &anthropic(&cut, true), None).unwrap(), Step::Rest);
    assert_eq!(g.partial(), Some(5));
    assert!(g.narrative().is_none());

    let body = g.request("sk-test", &[]).unwrap().body.unwrap();
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("**Slot shapes (Scaena)**"), "the slides' own prompt");
    assert!(prompt.contains("cut off after slide 5 of 8"));
    assert!(prompt.contains("5. Slide 5 (beat: Problem)\n"));
    assert!(prompt.contains("Write ONLY the remaining slides, 6 through 8"));

    // More than the length asks for is held to it.
    let rest = serde_json::json!({ "executiveSummary": "", "slides": slides(6, 10) }).to_string();
    assert_eq!(g.answer(200, "OK", &anthropic(&rest, false), None).unwrap(), Step::Done);
    let narrative = g.narrative().unwrap().clone();
    assert_eq!(narrative.slides.len(), 8);
    assert_eq!(
        (narrative.slides[5].slide_title.as_str(), narrative.slides[7].slide_title.as_str()),
        ("Slide 6", "Slide 8")
    );
    assert_eq!(narrative.executive_summary, "Cargo, carbon-negative.");
    assert!(!narrative.truncated, "the rest came back whole");

    let mut again = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    again.choose_arc(Some("Problem\u{2013}Solution"));
    again.written(g.written_text().unwrap(), false).unwrap();
    assert_eq!(again.narrative().unwrap().slides, narrative.slides);
}

/// A rest cut short again is asked for once more, then the deck stands as far as it was written;
/// a rest that fails leaves it so, saying why, and slides written elsewhere ask for no rest.
#[test]
fn the_rest_is_asked_twice_at_most_and_a_failure_keeps_what_was_written() {
    let start = |g: &mut Generation| {
        g.choose_arc(Some("Problem\u{2013}Solution"));
        let text = serde_json::json!({ "slides": slides(1, 5) }).to_string();
        g.answer(200, "OK", &anthropic(&text, true), None).unwrap()
    };
    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    assert_eq!(start(&mut g), Step::Rest);
    let some = |from, to| anthropic(&serde_json::json!({ "slides": slides(from, to) }).to_string(), true);
    assert_eq!(g.answer(200, "OK", &some(6, 6), None).unwrap(), Step::Rest);
    assert!(g.request("sk", &[]).unwrap().body.unwrap().to_string().contains("cut off after slide 6 of 8"));
    assert_eq!(g.answer(200, "OK", &some(7, 7), None).unwrap(), Step::Done);
    assert_eq!(g.narrative().unwrap().slides.len(), 7);
    assert!(g.narrative().unwrap().truncated);
    assert_eq!(g.unrested(), None);

    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    start(&mut g);
    let refused = r#"{"type":"error","error":{"type":"invalid_request_error","message":"too long"}}"#;
    assert_eq!(g.answer(400, "Bad Request", refused, None).unwrap(), Step::Done);
    assert_eq!(g.narrative().unwrap().slides.len(), 5);
    assert!(g.narrative().unwrap().truncated);
    assert!(g.unrested().unwrap().contains("too long"), "{:?}", g.unrested());

    let mut g = Generation::new(brief(), settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Problem\u{2013}Solution"));
    let cut = serde_json::json!({ "slides": slides(1, 5) }).to_string();
    assert_eq!(g.written(&cut, true).unwrap(), Step::Done, "slides written elsewhere ask for no rest");
}

/// A deck the rest leaves too short for its length is refused, and the slides are asked again.
#[test]
fn a_rest_too_short_for_the_length_asks_the_slides_again() {
    let mut g =
        Generation::new(Brief { length: "11-15 slides".into(), ..brief() }, settings(Provider::Anthropic)).unwrap();
    g.choose_arc(Some("Problem\u{2013}Solution"));
    let text = serde_json::json!({ "slides": slides(1, 4) }).to_string();
    assert_eq!(g.answer(200, "OK", &anthropic(&text, true), None).unwrap(), Step::Rest);
    let failed = g.answer(400, "Bad Request", r#"{"error":{"message":"no"}}"#, None);
    assert!(matches!(failed, Err(NarratoError::Incomplete { delivered: 4, minimum: 11 })), "{failed:?}");
    assert_eq!(g.step(), Step::Slides);
}
