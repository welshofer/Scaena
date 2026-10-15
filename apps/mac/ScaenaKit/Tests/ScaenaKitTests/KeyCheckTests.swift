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

/// The provider's answer to the request that lists its models says whether the key works: a list
/// is a key that works, a refusal is said in the provider's words, and an error of the provider's
/// own or no connection is neither.
@Test func theProvidersAnswerSaysWhetherTheKeyWorks() {
    let listed = #"{"data": [{"id": "claude-a"}, {"id": "claude-b"}]}"#
    #expect(KeyCheck.verdict(.anthropic, status: 200, body: listed) == .accepted(models: 2))
    let anthropic = #"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#
    #expect(
        KeyCheck.verdict(.anthropic, status: 401, body: anthropic)
            == .refused("Anthropic refused this key: invalid x-api-key"))
    let gemini = #"{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT"}}"#
    #expect(
        KeyCheck.verdict(.gemini, status: 400, body: gemini)
            == .refused("Gemini refused this key: API key not valid. Please pass a valid API key."))
    #expect(KeyCheck.verdict(.openai, status: 403, body: "") == .refused("OpenAI refused this key."))
    #expect(KeyCheck.verdict(.openai, status: 429, body: "") == .accepted(models: nil))
    #expect(KeyCheck.verdict(.openai, status: 429, body: "").works)
    let busy = KeyCheck.verdict(.openai, status: 503, body: "")
    #expect(!busy.works)
    if case .refused = busy { Issue.record("an error of OpenAI's own is not a refusal: \(busy)") }
}

/// A key is checked by the request that lists its models, with the key where its provider reads
/// it; a request that cannot be made is not a refusal.
@Test func aKeyIsCheckedByTheRequestThatListsItsModels() async {
    let listed = #"{"data": [{"id": "claude-a"}]}"#
    let asked = await KeyCheck.check(" sk-ant-test \n", for: .anthropic) { request in
        #expect(request.url?.path == "/v1/models")
        #expect(request.value(forHTTPHeaderField: "x-api-key") == "sk-ant-test")
        #expect(request.timeoutInterval <= 20)
        return (Data(listed.utf8), 200)
    }
    #expect(asked == .accepted(models: 1))
    let offline = await KeyCheck.check("sk-ant-test", for: .anthropic) { _ in throw URLError(.notConnectedToInternet) }
    if case .unreachable = offline {} else { Issue.record("no connection is not a key that works or one refused: \(offline)") }
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
            #expect(!why.contains(where: \.isNumber) && !why.contains("invalid"), "\(why)")
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
