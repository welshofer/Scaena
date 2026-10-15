import Foundation
import ScaenaKit
import Testing

/// A key kept shows whose it is and which, and nothing more of it.
@Test func aKeptKeyShowsWhoseItIsAndWhichAlone() {
    #expect(KeyCheck.masked("sk-ant-api03-abcdefghijklmnopqrstuvwxyz-1234") == "sk-ant-…1234")
    #expect(KeyCheck.masked("AIzaSyA1234567890abcdefghijklmnopqrstu") == "AIzaSyA…rstu")
    #expect(KeyCheck.masked("  sk-proj-abcdefghijklmnop9876\n") == "sk-proj…9876")
    #expect(KeyCheck.masked("short") == "•••••")
}

/// A key of another form is said so as it is typed, before it is sent; one of the right form is not.
@Test func aKeyOfAnotherFormIsSaidSoBeforeItIsSent() {
    #expect(KeyCheck.hint("sk-ant-api03-abcdefghijklmnopqrstuvwxyz", for: .anthropic) == nil)
    #expect(KeyCheck.hint("sk-proj-abcdefghijklmnopqrstuvwxyz", for: .openai) == nil)
    #expect(KeyCheck.hint("AIzaSyA1234567890abcdefghijklmnopqrstu", for: .gemini) == nil)
    #expect(KeyCheck.hint("", for: .anthropic) == nil)
    #expect(KeyCheck.hint("sk-proj-abcdefghijklmnopqrstuvwxyz", for: .anthropic) == "This looks like an OpenAI key.")
    #expect(KeyCheck.hint("sk-ant-api03-abcdefghijklmnopqrstuvwxyz", for: .openai) == "This looks like an Anthropic key.")
    #expect(KeyCheck.hint("hello-there-friend-of-mine", for: .gemini) == "Gemini's keys start with AIza.")
    #expect(KeyCheck.hint("sk-ant-api03 abcdefghijklmnop", for: .anthropic) == "A key has no spaces or line breaks in it.")
    #expect(KeyCheck.hint("sk-ant-abc", for: .anthropic) == "That is shorter than Anthropic's keys are.")
}

/// The provider's answer to the request that lists its models says whether the key works, as the
/// window says it (PLAN 3.18): a list is a key that works; a key not accepted, or an account out of
/// credit or without the models, is refused; the provider busy or in trouble is neither. No status,
/// and none of the provider's own words.
@Test func theProvidersAnswerSaysWhetherTheKeyWorks() {
    let listed = #"{"data": [{"id": "claude-a"}, {"id": "claude-b"}]}"#
    #expect(KeyCheck.verdict(.anthropic, status: 200, body: listed) == .accepted(models: 2))
    let anthropic = #"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#
    #expect(
        KeyCheck.verdict(.anthropic, status: 401, body: anthropic)
            == .refused("Anthropic didn't accept your key. Check it, or replace it."))
    let gemini = #"{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT"}}"#
    #expect(
        KeyCheck.verdict(.gemini, status: 400, body: gemini)
            == .refused("Gemini didn't accept your key. Check it, or replace it."))
    #expect(
        KeyCheck.verdict(.openai, status: 403, body: "") == .refused("OpenAI didn't accept your key. Check it, or replace it."))
    let credit = #"{"error": {"message": "Your credit balance is too low to access the Anthropic API."}}"#
    #expect(
        KeyCheck.verdict(.anthropic, status: 400, body: credit)
            == .refused("Your Anthropic account is out of credit, or past its limit. Check its plan and billing."))
    let permission = #"{"error": {"type": "permission_error", "message": "Your API key does not have permission to use the specified resource."}}"#
    #expect(
        KeyCheck.verdict(.anthropic, status: 403, body: permission)
            == .refused("Your Anthropic account can't use these models. Check its plan, or choose another model."))
    #expect(KeyCheck.verdict(.openai, status: 429, body: "") == .accepted(models: nil))
    #expect(KeyCheck.verdict(.openai, status: 429, body: "").works)
    let busy = KeyCheck.verdict(.openai, status: 503, body: "")
    #expect(!busy.works)
    if case .refused = busy { Issue.record("an error of OpenAI's own is not a refusal: \(busy)") }
    #expect(busy == .unreachable("OpenAI couldn't answer just now. Try again in a moment."))
}

/// A key is checked by the request that lists its models, with the key where its provider reads
/// it; a request that cannot be made is not a refusal.
@Test func aKeyIsCheckedByTheRequestThatListsItsModels() async {
    let listed = #"{"data": [{"id": "claude-a"}]}"#
    let asked = await KeyCheck.check(" sk-ant-test \n", for: .anthropic, environment: [:]) { request in
        #expect(request.url?.path == "/v1/models")
        #expect(request.value(forHTTPHeaderField: "x-api-key") == "sk-ant-test")
        #expect(request.timeoutInterval <= 20)
        return (Data(listed.utf8), 200)
    }
    #expect(asked == .accepted(models: 1))
    let offline = await KeyCheck.check("sk-ant-test", for: .anthropic, environment: [:]) { _ in
        throw URLError(.notConnectedToInternet)
    }
    if case .unreachable = offline {} else { Issue.record("no connection is not a key that works or one refused: \(offline)") }
}

/// What the key's check says, answered by stand-ins in the provider's place, is said in a person's
/// words (PLAN 3.18): a key refused, the provider busy, the provider in trouble, and no connection,
/// each with no status and none of the provider's or the network's own words.
@Test func aKeysCheckIsSaidInAPersonsWords() async {
    let refusal = #"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#
    let answers: [(Int, String, KeyCheck.Verdict)] = [
        (401, refusal, .refused("Anthropic didn't accept your key. Check it, or replace it.")),
        (429, #"{"error": {"type": "rate_limit_error", "message": "Number of requests has exceeded your rate limit"}}"#, .accepted(models: nil)),
        (500, #"{"error": {"type": "api_error", "message": "Internal server error"}}"#, .unreachable("Anthropic couldn't answer just now. Try again in a moment.")),
    ]
    for (status, body, verdict) in answers {
        let checked = await KeyCheck.check("sk-ant-test", for: .anthropic, environment: [:]) { _ in (Data(body.utf8), status) }
        #expect(checked == verdict, "answered \(status)")
    }
    let offline = await KeyCheck.check("sk-ant-test", for: .anthropic, environment: [:]) { _ in
        throw URLError(.notConnectedToInternet)
    }
    #expect(offline == .unreachable("Anthropic can't be reached. Check your internet connection."))
    let timedOut = await KeyCheck.check("AIza-test", for: .gemini, environment: [:]) { _ in throw URLError(.timedOut) }
    #expect(timedOut == .unreachable("Gemini can't be reached. Check your internet connection."))

    // The stand-ins the apps' UI tests use say the same, and send the key nowhere.
    let unsent: Assistant.Send = { request in
        Issue.record("the key was sent to \(request.url?.absoluteString ?? "a provider")")
        return (Data(), 500)
    }
    let refused = await KeyCheck.check("sk-test", for: .openai, environment: ["SCAENA_KEY_CHECK": "refuse"], send: unsent)
    #expect(refused == .refused("OpenAI didn't accept your key. Check it, or replace it."))
    let unreached = await KeyCheck.check("sk-test", for: .openai, environment: ["SCAENA_KEY_CHECK": "offline"], send: unsent)
    #expect(unreached == .unreachable("OpenAI can't be reached. Check your internet connection."))
}

/// The models a kept key can use, as the assistant and New from a Prompt list them; where none are
/// listed, why, as the window says it (PLAN 3.18): the key not accepted, the provider busy or in
/// trouble, or no way to it, with no status and none of the provider's own words.
@Test func whyNoModelsAreListedIsSaidInAPersonsWords() async {
    let refusal = #"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#
    let notAccepted = KeyCheck.Listing.unlisted("Anthropic didn't accept your key. Check it, or replace it.")
    let answers: [(Int, String, KeyCheck.Listing)] = [
        (200, #"{"data": [{"id": "model-a"}, {"id": "model-b"}]}"#, .listed(["model-a", "model-b"])),
        (401, refusal, notAccepted),
        (403, "", notAccepted),
        (429, "", .unlisted("Anthropic is busy. Try again in a moment.")),
        (529, #"{"error": {"message": "Overloaded"}}"#, .unlisted("Anthropic couldn't answer just now. Try again in a moment.")),
        (200, "<html>Sign in to the network</html>", .unlisted("Anthropic's models couldn't be listed. Try again in a moment.")),
    ]
    for (status, body, said) in answers {
        let listed = await KeyCheck.models(" sk-ant-test \n", for: .anthropic, environment: [:]) { request in
            #expect(request.url?.path == "/v1/models")
            #expect(request.value(forHTTPHeaderField: "x-api-key") == "sk-ant-test")
            return (Data(body.utf8), status)
        }
        #expect(listed == said, "answered \(status)")
        if case .unlisted(let why) = listed {
            // Worked out before `#expect`, which expands a call given a key path as one that throws.
            let digits = why.contains(where: \.isNumber)
            #expect(!digits && !why.contains("invalid"), "\(why)")
        }
    }
    let gemini = #"{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key."}}"#
    let refused = await KeyCheck.models("AIza-test", for: .gemini, environment: [:]) { _ in (Data(gemini.utf8), 400) }
    #expect(refused == .unlisted("Gemini didn't accept your key. Check it, or replace it."))
    let offline = await KeyCheck.models("sk-ant-test", for: .anthropic, environment: [:]) { _ in
        throw URLError(.notConnectedToInternet)
    }
    #expect(offline == .unlisted("Anthropic can't be reached. Check your internet connection."))
}

/// For the apps' UI tests, `SCAENA_KEY_CHECK` answers the listing in the provider's place, as it
/// answers the check: the models listed, the key refused, or no connection, and the key sent
/// nowhere.
@Test func theStandInListsTheModelsAndSendsTheKeyNowhere() async {
    let unsent: Assistant.Send = { request in
        Issue.record("the key was sent to \(request.url?.absoluteString ?? "a provider")")
        return (Data(), 500)
    }
    let standIn = { (answer: String) in ["SCAENA_KEY_CHECK": answer] }
    let accepted = await KeyCheck.models("sk-ant-test", for: .anthropic, environment: standIn("accept"), send: unsent)
    #expect(accepted == .listed([]))
    let refused = await KeyCheck.models("sk-ant-test", for: .anthropic, environment: standIn("refuse"), send: unsent)
    #expect(refused == .unlisted("Anthropic didn't accept your key. Check it, or replace it."))
    let offline = await KeyCheck.models("sk-ant-test", for: .anthropic, environment: standIn("offline"), send: unsent)
    #expect(offline == .unlisted("Anthropic can't be reached. Check your internet connection."))
}
