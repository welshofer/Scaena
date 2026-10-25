//! A deck made from a prompt through the C ABI, step by step as ScaenaKit's `Generator` makes one
//! (PLAN G.6): each request answered here with what Anthropic recorded, the slides' pictures made
//! by a stand-in for OpenAI's images API (PLAN G.10), and the bundle opened as a session with a
//! state for each slide and each picture among its files. Each key goes in its own provider's
//! header, and nowhere else.

use base64::Engine;
use scaena_ffi::*;
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::ptr::{null, null_mut};

const ANSWERS: &str = "../scaena-narrato/tests/answers";

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// A string the library returned, taken and freed.
fn took(p: *mut c_char) -> String {
    assert!(!p.is_null(), "a string was returned");
    let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string();
    unsafe { scaena_string_free(p) };
    s
}

/// One step of the generation as it answers: `{"ok"}` or `{"error"}`.
fn answered(g: *mut ScaenaGenerator, method: &str, args: &Value, bytes: &[u8]) -> Value {
    let (method, args) = (c(method), c(&args.to_string()));
    let at = if bytes.is_empty() { null() } else { bytes.as_ptr() };
    let answer = took(unsafe { scaena_generator_call(g, method.as_ptr(), args.as_ptr(), at, bytes.len()) });
    serde_json::from_str(&answer).unwrap()
}

/// One step answered: its `ok`, or a panic with its error.
fn step(g: *mut ScaenaGenerator, method: &str, args: Value, bytes: &[u8]) -> Value {
    let answer = answered(g, method, &args, bytes);
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{method}: {answer}"))
}

/// A session's call answered: its `ok`, or a panic with its error.
fn call(s: *mut ScaenaSession, method: &str) -> Value {
    let (method, args) = (c(method), c("{}"));
    let answer: Value = serde_json::from_str(&took(unsafe { scaena_call(s, method.as_ptr(), args.as_ptr()) })).unwrap();
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{answer}"))
}

/// An MCP operation run on a session by the user, as ScaenaKit's `tool` runs one.
fn tool(s: *mut ScaenaSession, name: &str, args: Value) -> Value {
    let (name, args, author) = (c(name), c(&args.to_string()), c("user"));
    let called = unsafe { scaena_tool(s, name.as_ptr(), args.as_ptr(), author.as_ptr(), null()) };
    serde_json::from_str(&took(called)).unwrap()
}

/// An Anthropic answer whose text is `text`.
fn anthropic(text: &str) -> String {
    json!({ "content": [{ "type": "text", "text": text }], "stop_reason": "end_turn" }).to_string()
}

#[test]
fn a_deck_is_made_from_a_prompt_as_swift_makes_one_its_pictures_with_it() {
    let arc = std::fs::read_to_string(format!("{ANSWERS}/anthropic-arc.json")).unwrap();
    let slides = std::fs::read_to_string(format!("{ANSWERS}/anthropic-slides.json")).unwrap();
    let jpeg = std::fs::read("../../tests/fixtures/jpeg/baseline-422.jpg").unwrap();
    let image = json!({ "data": [{ "b64_json": base64::engine::general_purpose::STANDARD.encode(&jpeg) }] });
    let image = image.to_string();
    let g = scaena_generator_new();

    // A source read from its bytes, as Swift hands a file over: more than Narrato's 100
    // characters, so that it grounds the slides.
    let notes = "# Pilots\n\nThree lanes ran carbon-negative for a quarter. Fuel cost rose 4%, and every shipper renewed its contract for the next season.\n";
    let read = step(g, "add", json!({ "name": "notes.md" }), notes.as_bytes());
    assert_eq!(read, json!({ "name": "notes.md", "kind": "markdown" }));

    // The brief as Swift encodes `Brief`: what is left empty is left out.
    let brief = json!({ "topic": "Carbon-negative cargo is ready to scale", "audience": "Investors" });
    step(g, "begin", json!({ "brief": brief, "provider": "anthropic", "model": "a-model" }), &[]);
    let mut asked = Vec::new();
    loop {
        let standing = step(g, "request", json!({ "key": "sk-writer" }), &[]);
        let Some(request) = standing.get("request").cloned() else { break };
        let body = request["body"].as_str().unwrap();
        let answer = if body.contains("=== CATALOG ===") { &arc } else { &slides };
        step(g, "answer", json!({ "status": 200, "statusText": "OK", "body": answer }), &[]);
        asked.push(request);
    }
    assert_eq!(asked.len(), 2, "the arc, then the slides");
    let written = asked[1]["body"].as_str().unwrap();
    assert!(written.contains("every shipper renewed its contract"), "the source grounds the slides");

    // The pictures: the key's image models listed, the best first, every picture asked at once.
    let listing = step(g, "imageModels", json!({ "provider": "openai", "key": "sk-pictures" }), &[]);
    assert_eq!(listing["request"]["url"], "https://api.openai.com/v1/models");
    let listed = json!({ "data": [{ "id": "gpt-4o" }, { "id": "dall-e-3" }, { "id": "gpt-image-1" }] }).to_string();
    let models = step(g, "imageModelsRead", json!({ "provider": "openai", "status": 200, "body": listed }), &[]);
    assert_eq!(models["models"][0], "gpt-image-1");
    let pictures = json!({ "theme": "dusk", "look": "illustration", "provider": "openai", "model": "gpt-image-1" });
    let wanted = step(g, "pictures", pictures, &[])["pictures"].as_u64().unwrap();
    assert!(wanted > 0, "the slides ask for pictures");
    let mut painted = Vec::new();
    loop {
        let round = step(g, "paint", json!({ "key": "sk-pictures" }), &[]);
        let requests = round["requests"].as_array().unwrap().clone();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            // Each answer handed over as its bytes, as `URLSession` reads them.
            step(g, "painted", json!({ "status": 200, "statusText": "OK" }), image.as_bytes());
            painted.push(request);
        }
        assert_eq!(step(g, "round", json!({}), &[])["wait"], 0.0);
    }
    assert_eq!(painted.len() as u64, wanted, "each picture asked for once");
    for request in &painted {
        assert_eq!(request["url"], "https://api.openai.com/v1/images/generations");
        assert_eq!(request["headers"]["authorization"], "Bearer sk-pictures");
        let body: Value = serde_json::from_str(request["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["model"], "gpt-image-1");
        assert!(body["prompt"].as_str().unwrap().contains("NO text"), "{body}");
    }
    let finished = step(g, "finish", json!({}), &[]);
    assert_eq!(finished, json!({ "pictures": wanted, "unmade": [] }));

    // The deck made in Dusk and shown to the model as it is drawn (PLAN G.13): each slide a JPEG
    // after what is said of it.
    let making = step(g, "make", json!({ "theme": "dusk", "key": "sk-writer" }), &[]);
    let look = &making["look"];
    assert_eq!(look["rounds"].as_array().unwrap().len(), 1, "a short deck's look is one round");
    assert_eq!(look["atOnce"], 3);
    let round = &look["rounds"][0];
    assert_eq!(round["headers"]["x-api-key"], "sk-writer");
    let body: Value = serde_json::from_str(round["body"].as_str().unwrap()).unwrap();
    let content = body["messages"][0]["content"].as_array().unwrap();
    let drawn: Vec<&Value> = content.iter().filter(|c| c["type"] == "image").collect();
    assert_eq!(drawn.len() as u64, look["slides"].as_u64().unwrap());
    assert!(drawn.iter().all(|d| d["source"]["media_type"] == "image/jpeg"));
    assert!(content[0]["text"].as_str().unwrap().starts_with("Slide 1 of "), "{}", content[0]);
    assert!(body["system"][0]["text"].as_str().unwrap().contains("art director"));
    // The model tightens the cover's title; then a text that runs past its place is asked for
    // shorter.
    let tightened = anthropic(
        &json!({ "slides": [{ "slide": 1, "layout": "", "title": "Carbon-negative cargo, ready to scale", "subtitle": "", "bullets": [], "why": "One idea, one line." }] })
            .to_string(),
    );
    let answered = json!([{ "status": 200, "statusText": "OK", "body": tightened }]);
    let looked = step(g, "looked", json!({ "replies": answered, "key": "sk-writer" }), &[]);
    assert_eq!(looked["looked"], 1, "{looked}");
    // Each rewrite asked for answered with none: asked once more, no more, then said (STAB-4).
    let (mut fit, mut rounds) = (looked["fit"].clone(), 0);
    while let Some(asked) = fit.as_object() {
        assert_eq!(asked["request"]["headers"]["x-api-key"], "sk-writer");
        rounds += 1;
        let none = anthropic(r#"{"texts": []}"#);
        let fitted =
            step(g, "fitted", json!({ "status": 200, "statusText": "OK", "body": none, "key": "sk-writer" }), &[]);
        assert_eq!(fitted["fitted"], 0, "{fitted}");
        fit = fitted["fit"].clone();
    }
    assert!(rounds <= 2, "{rounds} rounds");
    let report = step(g, "report", json!({}), &[]);
    assert_eq!(report["created"]["created"], true, "{report:#}");
    assert_eq!(report["created"]["errors"], 0, "{report:#}");
    if rounds == 2 {
        assert!(report["unfitted"].as_str().unwrap().contains("still run"), "{report:#}");
    }
    assert_eq!(report["pictures"].as_u64(), Some(wanted), "{report:#}");
    assert_eq!(report["arc"], "Problem–Solution");
    assert_eq!(report["sources"][0]["name"], "notes.md");
    assert_eq!(report["looked"][0]["title"], "Carbon-negative cargo, ready to scale", "{report:#}");

    let mut error = null_mut();
    let session = unsafe { scaena_generator_open(g, &mut error) };
    assert!(!session.is_null(), "the deck opens: {}", took(error));
    let states = call(session, "states");
    assert_eq!(states.as_array().unwrap().len(), report["layouts"].as_array().unwrap().len());
    let files = call(session, "bundleFiles");
    let kept: Vec<&str> = (files.as_array().unwrap().iter())
        .filter_map(|f| f["path"].as_str())
        .filter(|p| p.starts_with("assets/picture-"))
        .collect();
    assert_eq!(kept.len() as u64, wanted, "{files:#}");
    assert!(kept.iter().all(|p| p.ends_with(".jpg")), "a JPEG stays a JPEG: {kept:?}");
    // Its story, as the Mac's Story panel reads it (PLAN G.9): the arc it was written to, each slide
    // read, coherence by words unscored, and each finding said in words; no fix rewrites the plan,
    // and a moved slide's fix made by the user, as the panel makes one, and its finding gone.
    let health = call(session, "health");
    assert_eq!(health["arc"], "Problem–Solution", "{health:#}");
    assert_eq!(health["slides"].as_array().unwrap().len(), report["layouts"].as_array().unwrap().len());
    assert!(health["coherence"]["score"].is_null(), "{health:#}");
    assert!(health["slides"].as_array().unwrap().iter().all(|s| s["declared"].is_number()), "{health:#}");
    let findings = health["findings"].as_array().unwrap();
    assert!(findings.iter().all(|f| f["says"].as_str().is_some_and(|s| s.starts_with("Slide "))));
    // The plan's intensity and tone are the words' to carry: no fix declares what they read.
    let planned = |f: &&Value| (f["kind"] == "intensity" || f["kind"] == "tone") && f["fix"].is_object();
    assert!(!findings.iter().any(|f| planned(&f)), "{health:#}");
    // A slide moved out of its beat's order has drifted, and its fix makes it part of the beat it
    // now stands in.
    let moved =
        tool(session, "deck_patch", json!({ "ops": [{ "op": "move", "from": "/states/1", "path": "/states/2" }] }));
    assert_eq!(moved["edited"], true, "{moved}");
    let health = call(session, "health");
    let findings = health["findings"].as_array().unwrap();
    let fixable = findings.iter().find(|f| f["fix"].is_object()).expect("a finding with a fix");
    assert_eq!(fixable["kind"], "drift", "{health:#}");
    let applied = tool(session, "deck_patch", json!({ "ops": fixable["fix"]["patch"] }));
    assert_eq!(applied["edited"], true, "{applied}");
    let after = call(session, "health");
    let gone = |f: &Value| f["state"] == fixable["state"] && f["kind"] == fixable["kind"];
    assert!(!after["findings"].as_array().unwrap().iter().any(gone), "{after:#}");

    // Each key went to its own provider alone.
    for request in &asked {
        assert_eq!(request["headers"]["x-api-key"], "sk-writer");
        assert!(
            !["sk-writer", "sk-pictures"].iter().any(|k| request["body"].as_str().unwrap().contains(k)),
            "no key in a body"
        );
    }
    for request in &painted {
        assert!(request["headers"].get("x-api-key").is_none());
        assert!(
            !["sk-writer", "sk-pictures"].iter().any(|k| request["body"].as_str().unwrap().contains(k)),
            "no key in a body"
        );
    }
    unsafe {
        scaena_session_free(session);
        scaena_generator_free(g);
    }
}

/// The icons the slides name are found in the set that ships as the deck is made, as the CLI
/// finds them (PLAN G.23): a slide whose items each name one sets each under it, and Swift does
/// nothing more for it.
/// A busy provider's answer to the look or the fit, as Swift's loop hands it over: the same
/// request handed back with the wait to sleep and the tries so far, and a refusal after it left
/// as it is (lift-up REL-1).
#[test]
fn a_busy_look_or_fit_is_handed_back_to_ask_again_as_swift_asks_it() {
    let arc = std::fs::read_to_string(format!("{ANSWERS}/anthropic-arc.json")).unwrap();
    // The slides as recorded, the second's title made far too long for its place, so that the
    // deck made asks for the fit.
    let mut slides: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{ANSWERS}/anthropic-slides.json")).unwrap()).unwrap();
    let block = (slides["content"].as_array_mut().unwrap().iter_mut()).find(|c| c["type"] == "text").unwrap();
    let text = block["text"].as_str().unwrap().to_string();
    let mut written: Value = serde_json::from_str(&text[text.find('{').unwrap()..=text.rfind('}').unwrap()]).unwrap();
    // As the ops test makes one: an agenda's headline stands in a narrow column, which this title
    // runs past at any size.
    written["slides"][1]["layoutSuggestion"] = json!("Agenda");
    written["slides"][1]["slideTitle"] =
        json!("What we will cover today across the whole Gulf Coast and every port beyond it");
    block["text"] = json!(written.to_string());
    let slides = slides.to_string();
    let g = scaena_generator_new();
    let brief = json!({ "topic": "Carbon-negative cargo is ready to scale", "audience": "Investors" });
    step(g, "begin", json!({ "brief": brief, "provider": "anthropic", "model": "a-model" }), &[]);
    loop {
        let standing = step(g, "request", json!({ "key": "sk-writer" }), &[]);
        let Some(request) = standing.get("request").cloned() else { break };
        let answer = if request["body"].as_str().unwrap().contains("=== CATALOG ===") { &arc } else { &slides };
        step(g, "answer", json!({ "status": 200, "statusText": "OK", "body": answer }), &[]);
    }
    let making = step(g, "make", json!({ "theme": "dusk", "key": "sk-writer" }), &[]);
    let first = making["look"]["rounds"][0].clone();
    assert!(first.is_object(), "{making}");
    let overloaded = json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } });
    let busy = json!({ "status": 529, "statusText": "", "body": overloaded.to_string(), "after": "1" });
    let at_529 =
        json!({ "status": 529, "statusText": "", "body": overloaded.to_string(), "after": "1", "key": "sk-writer" });

    // The look: busy, then the same round handed back to ask again; a refusal then ends it.
    let again = step(g, "looked", json!({ "replies": [busy], "key": "sk-writer" }), &[]);
    assert_eq!((again["wait"].as_f64(), again["tried"].as_u64()), (Some(1.0), Some(1)), "{again}");
    assert_eq!(again["look"]["rounds"], json!([first]), "the round first handed out");
    let refused = json!({ "type": "error", "error": { "type": "invalid_request_error", "message": "Bad" } });
    let bad = json!({ "status": 400, "statusText": "Bad Request", "body": refused.to_string() });
    let ended = step(g, "looked", json!({ "replies": [bad], "key": "sk-writer" }), &[]);
    assert!(ended["unlooked"].as_str().is_some_and(|w| w.contains("not looked at")), "{ended}");
    assert!(ended.get("wait").is_none(), "a refusal is not asked again: {ended}");

    // The fit: busy, then the same request handed back; answered, the text fits.
    let fit = ended["fit"]["request"].clone();
    assert!(fit.is_object(), "the long title asks for the fit: {ended}");
    let again = step(g, "fitted", at_529, &[]);
    assert_eq!((again["wait"].as_f64(), again["tried"].as_u64()), (Some(1.0), Some(1)), "{again}");
    assert_eq!(again["fit"]["request"], fit, "the fit first handed out");
    let prompt: Value = serde_json::from_str(fit["body"].as_str().unwrap()).unwrap();
    let prompt = prompt["messages"][0]["content"][0]["text"].as_str().unwrap().to_string();
    let id = prompt.split("id: ").nth(1).unwrap().split_once('\n').unwrap().0;
    let rewrite = anthropic(&json!({ "texts": [{ "id": id, "text": "What we will cover" }] }).to_string());
    let fitted = step(g, "fitted", json!({ "status": 200, "statusText": "OK", "body": rewrite }), &[]);
    assert_eq!(fitted["fitted"], 1, "{fitted}");
    let report = step(g, "report", json!({}), &[]);
    assert_eq!(report["fitted"], json!([id]), "{report:#}");
}

#[test]
fn the_icons_the_slides_name_are_found_as_the_deck_is_made() {
    let arc = std::fs::read_to_string(format!("{ANSWERS}/anthropic-arc.json")).unwrap();
    let three = ["Fast: a crossing in a day", "Safe: checked at every port", "Shared: one ledger for all"];
    let four = ["Plan: the route set a week out", "Track: each crate seen", "Agree: one price", "Arrive: on time"];
    let written = json!({ "executiveSummary": "Cargo that pays for its carbon.", "slides": [
        { "slideTitle": "Tidewater", "subtitle": "Series A", "layoutSuggestion": "Title", "keyBullets": [] },
        { "slideTitle": "Why Tidewater wins", "layoutSuggestion": "Bullet List", "keyBullets": three, "icons": ["lightning", "shield", "users"] },
        { "slideTitle": "Cargo can pay for the carbon it moves", "layoutSuggestion": "Statement", "keyBullets": [] },
        { "slideTitle": "How we work", "layoutSuggestion": "Bullet List", "keyBullets": four, "icons": ["compass", "chart", "handshake", "flag"] },
        { "slideTitle": "Sail with us", "layoutSuggestion": "Closing", "keyBullets": [] },
    ] });
    let slides = anthropic(&written.to_string());
    let g = scaena_generator_new();
    let brief = json!({ "topic": "Carbon-negative cargo is ready to scale", "audience": "Investors" });
    step(g, "begin", json!({ "brief": brief, "provider": "anthropic", "model": "a-model" }), &[]);
    loop {
        let standing = step(g, "request", json!({ "key": "sk-writer" }), &[]);
        let Some(request) = standing.get("request").cloned() else { break };
        let answer = if request["body"].as_str().unwrap().contains("=== CATALOG ===") { &arc } else { &slides };
        step(g, "answer", json!({ "status": 200, "statusText": "OK", "body": answer }), &[]);
    }
    step(g, "make", json!({ "theme": "dusk" }), &[]);
    let report = step(g, "report", json!({}), &[]);
    assert_eq!(report["created"]["errors"], 0, "{report:#}");
    assert_eq!(report["layouts"][1], "icon-cards", "{report:#}");
    assert_eq!(report["layouts"][3], "icon-rows", "{report:#}");
    let mut error = null_mut();
    let session = unsafe { scaena_generator_open(g, &mut error) };
    assert!(!session.is_null(), "the deck opens: {}", took(error));
    let source = call(session, "source");
    let source = source.as_str().unwrap();
    for named in ["alt:Lightning", "alt:Handshake", "view:[0, 0, 256, 256]"] {
        assert!(source.contains(named), "{named}: {source}");
    }
    unsafe {
        scaena_session_free(session);
        scaena_generator_free(g);
    }
}

/// A deck in a script the theme's fonts do not draw takes the script's font, which Swift fetches
/// where `script` says and hands over as `scriptFont`, held to its pinned digest (PLAN G.22). The
/// theme is asked before anything is read or any call paid for, and refuses the language until
/// the font is given, in a person's words.
#[test]
fn a_deck_in_another_script_takes_the_font_swift_fetches() {
    let g = scaena_generator_new();
    let asked = step(g, "script", json!({ "theme": "dusk", "language": "Hindi" }), &[]);
    let script = &asked["script"];
    assert_eq!(script["family"], "Noto Sans Devanagari");
    assert!(script["url"].as_str().unwrap().starts_with("https://raw.githubusercontent.com/google/fonts/"));
    let here = step(g, "script", json!({ "theme": "dusk", "language": "Hindi", "base": "http://127.0.0.1:9" }), &[]);
    assert!(here["script"]["url"].as_str().unwrap().starts_with("http://127.0.0.1:9/ofl/"));
    for language in ["Ukrainian", "Czech", "English", ""] {
        assert_eq!(step(g, "script", json!({ "theme": "ember", "language": language }), &[])["script"], Value::Null);
    }
    let answer = answered(g, "writable", &json!({ "theme": "dusk", "language": "Hindi" }), &[]);
    let message = answer["error"]["message"].as_str().unwrap_or_default();
    assert!(message.starts_with("a deck in Hindi cannot be made in Dusk without Noto Sans Devanagari"), "{answer}");
    assert!(!message.contains("PLAN"), "{message}");
    let answer = answered(g, "scriptFont", &json!({ "key": "devanagari" }), b"not the font");
    assert!(answer["error"]["message"].as_str().is_some_and(|m| m.contains("is not the font it names")), "{answer}");
    let font = std::fs::read("../../tests/fixtures/scripts/NotoSansDevanagari-VF.ttf").unwrap();
    step(g, "scriptFont", json!({ "key": "devanagari" }), &font);
    step(g, "writable", json!({ "theme": "dusk", "language": "Hindi" }), &[]);
    let answer = answered(g, "script", &json!({ "theme": "no such theme", "language": "German" }), &[]);
    assert!(answer["error"]["message"].as_str().is_some_and(|m| m.contains("not a theme that ships")), "{answer}");
    unsafe { scaena_generator_free(g) };
}

/// One picture a person asks for in an editor (PLAN 2.107), as ScaenaKit's `Generator.picture`
/// makes it: of their words, in the deck's theme, framed as its place, painted as a made deck's
/// pictures are, and handed back with what it shows; a refusal says why, and so does a picture
/// that shows nothing.
#[test]
fn a_picture_asked_for_in_an_editor_is_painted_and_handed_back() {
    let b64 = base64::engine::general_purpose::STANDARD;
    let g = scaena_generator_new();
    let theme = std::fs::read_to_string("../../docs/examples/themes/dusk.theme.json").unwrap();
    let picture = |subject: &str| {
        json!({ "theme": theme, "subject": subject, "look": "photo", "width": 400, "height": 900,
                "provider": "openai", "model": "gpt-image-1" })
    };
    assert_eq!(step(g, "picture", picture("A lighthouse at dusk"), &[])["pictures"], 1);
    let round = step(g, "paint", json!({ "key": "sk-pictures" }), &[]);
    let requests = round["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 1, "asked once");
    assert_eq!(requests[0]["headers"]["authorization"], "Bearer sk-pictures");
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["size"], "1024x1536", "framed tall, as its place is");
    let prompt = body["prompt"].as_str().unwrap();
    assert!(prompt.contains("SUBJECT: A lighthouse at dusk") && prompt.contains("#FF6A3D"), "{prompt}");
    let jpeg = std::fs::read("../../tests/fixtures/jpeg/baseline-422.jpg").unwrap();
    let answer = json!({ "data": [{ "b64_json": b64.encode(&jpeg) }] }).to_string();
    step(g, "painted", json!({ "status": 200, "statusText": "OK" }), answer.as_bytes());
    assert_eq!(step(g, "round", json!({}), &[])["wait"], 0.0);
    assert!(step(g, "paint", json!({ "key": "sk-pictures" }), &[])["requests"].as_array().unwrap().is_empty());
    let made = step(g, "pictureMade", json!({}), &[]);
    assert_eq!((made["alt"].as_str(), made["ext"].as_str()), (Some("A lighthouse at dusk"), Some("jpg")));
    assert_eq!(b64.decode(made["bytes"].as_str().unwrap()).unwrap(), jpeg, "the picture's own bytes");
    // Refused by the provider: why, and nothing handed back.
    step(g, "picture", picture("A harbor at night"), &[]);
    step(g, "paint", json!({ "key": "sk-pictures" }), &[]);
    let refused = br#"{"error":{"message":"Your request was rejected by the safety system."}}"#;
    step(g, "painted", json!({ "status": 400, "statusText": "Bad Request" }), refused);
    step(g, "round", json!({}), &[]);
    let none = answered(g, "pictureMade", &json!({}), &[]);
    let why = none["error"]["message"].as_str().unwrap();
    assert!(why.contains("safety system"), "{none}");
    assert!(!why.starts_with("slide"), "a picture asked for in an editor is for no slide of a made deck: {why}");
    // A picture of nothing is not asked for.
    let blank = answered(g, "picture", &picture("  "), &[]);
    assert!(blank["error"]["message"].as_str().unwrap().contains("say what the picture shows"), "{blank}");
    unsafe { scaena_generator_free(g) };
}

#[test]
fn a_step_out_of_order_is_an_error_that_says_why() {
    let g = scaena_generator_new();
    for (method, args) in [
        ("request", json!({ "key": "k" })),
        ("paint", json!({ "key": "k" })),
        ("round", json!({})),
        ("finish", json!({})),
        ("pictureMade", json!({})),
        ("make", json!({ "theme": "dusk" })),
        ("report", json!({})),
        ("no such step", json!({})),
    ] {
        let answer = answered(g, method, &args, &[]);
        assert!(answer["error"]["message"].as_str().is_some_and(|m| !m.is_empty()), "{method}: {answer}");
    }
    let mut error = null_mut();
    assert!(unsafe { scaena_generator_open(g, &mut error) }.is_null());
    assert!(took(error).contains("no deck is made"));
    unsafe { scaena_generator_free(g) };
}

/// A long deck's look is asked in every round at once (lift-up 20261025 PERF-2): `make` hands
/// over each round's request, which ScaenaKit sends three at a time as the CLI and the browser
/// do, and `looked` takes every answer back in the rounds' order, a round not answered losing
/// its own slides' changes alone.
#[test]
fn a_long_decks_look_hands_over_every_round_at_once() {
    let slides = std::fs::read_to_string(format!("{ANSWERS}/catalog.json")).unwrap();
    let g = scaena_generator_new();
    let brief = json!({ "topic": "Tidewater's Series A", "audience": "Investors" });
    step(g, "begin", json!({ "brief": brief, "provider": "anthropic", "model": "a-model" }), &[]);
    step(g, "chooseArc", json!({}), &[]);
    step(g, "written", json!({ "text": slides }), &[]);
    let making = step(g, "make", json!({ "theme": "dusk", "key": "sk-writer" }), &[]);
    let look = &making["look"];
    let rounds = look["rounds"].as_array().unwrap();
    let slides = look["slides"].as_u64().unwrap() as usize;
    assert_eq!(rounds.len(), scaena_ops::narrative::look_rounds(slides).count(), "{slides} slides");
    assert!(rounds.len() > 1, "{} rounds", rounds.len());
    // Each round shows its own slides, in order.
    let first = |r: &Value| {
        let body: Value = serde_json::from_str(r["body"].as_str().unwrap()).unwrap();
        body["messages"][0]["content"][0]["text"].as_str().unwrap().to_string()
    };
    assert!(first(&rounds[0]).starts_with("Slide 1 of "), "{}", first(&rounds[0]));
    assert!(!first(&rounds[1]).starts_with("Slide 1 of "), "{}", first(&rounds[1]));
    // Too few answers is refused. The first round answered with a change, the second busy for now
    // and the third refused, the rest with none: the busy round alone is handed back, after the
    // wait it asked, its tries counted (lift-up REL-1), and the rounds that answered are kept.
    let refused = answered(g, "looked", &json!({ "replies": [], "key": "sk-writer" }), &[]);
    assert!(
        refused["error"]["message"].as_str().unwrap().contains("answer each round handed over, in order"),
        "{refused}"
    );
    let tightened = anthropic(
        &json!({ "slides": [{ "slide": 1, "layout": "", "title": "Freight that pulls carbon", "subtitle": "", "bullets": [], "why": "Shorter." }] })
            .to_string(),
    );
    let busy = json!({ "status": 529, "statusText": "", "body": r#"{"error":{"message":"busy"}}"#, "after": "2" });
    let bad = json!({ "status": 400, "statusText": "Bad Request", "body": r#"{"error":{"message":"bad"}}"# });
    let mut replies = vec![json!({ "status": 200, "statusText": "OK", "body": tightened }), busy, bad];
    for _ in 3..rounds.len() {
        replies.push(json!({ "status": 200, "statusText": "OK", "body": anthropic(r#"{"slides": []}"#) }));
    }
    let again = step(g, "looked", json!({ "replies": replies }), &[]);
    assert_eq!((again["wait"].as_f64(), again["tried"].as_u64()), (Some(2.0), Some(1)), "{again}");
    assert_eq!(again["look"]["rounds"], json!([rounds[1]]), "the busy round alone, as first handed out");
    let none = json!({ "status": 200, "statusText": "OK", "body": anthropic(r#"{"slides": []}"#) });
    let looked = step(g, "looked", json!({ "replies": [none] }), &[]);
    assert_eq!(looked["looked"], 1, "the first round's change kept: {looked}");
    let unlooked = looked["unlooked"].as_str().unwrap();
    assert!(unlooked.starts_with("slides ") && unlooked.contains("not looked at"), "the refused round's: {unlooked}");
    assert_eq!(unlooked.matches("not looked at").count(), 1, "the busy round, answered, is not said: {unlooked}");
    unsafe { scaena_generator_free(g) };
}
