import Foundation
import ScaenaKit
import Testing

/// Whatever a provider answered in place of an answer is said by what it means for the person who
/// asked (PLAN 3.18): the key not accepted, the account out of credit or past its limit, or without
/// the models, the provider's safety system refusing, more than the model takes, the provider busy
/// or in trouble, no way to it, or an answer that does not read. No status, no code, and none of
/// the provider's own words reach the window.
@Test func aProvidersAnswerThatIsNoAnswerIsSaidByWhatItMeans() {
    let anthropic = { (status: Int, body: String) in Why.answered(.anthropic, status: status, body: body) }
    let refusal = #"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#
    #expect(anthropic(401, refusal) == "Anthropic didn't accept your key. Check it, or replace it.")
    #expect(anthropic(403, "") == "Anthropic didn't accept your key. Check it, or replace it.")
    let credit = #"{"error": {"message": "Your credit balance is too low to access the Anthropic API. Please go to Plans & Billing to upgrade or purchase credits."}}"#
    #expect(anthropic(400, credit) == "Your Anthropic account is out of credit, or past its limit. Check its plan and billing.")
    let permission = #"{"error": {"type": "permission_error", "message": "Your API key does not have permission to use the specified resource."}}"#
    #expect(anthropic(403, permission) == "Your Anthropic account can't use these models. Check its plan, or choose another model.")
    let unknown = #"{"type": "error", "error": {"type": "not_found_error", "message": "model: a-model"}}"#
    #expect(anthropic(404, unknown) == "Your Anthropic account can't use these models. Check its plan, or choose another model.")
    let long = #"{"error": {"message": "prompt is too long: 215000 tokens > 200000 maximum"}}"#
    #expect(anthropic(400, long) == "That is more than Anthropic's model takes at once. Ask about less, or start again.")
    #expect(anthropic(429, #"{"error": {"message": "Rate limit reached"}}"#) == "Anthropic is busy. Try again in a moment.")
    #expect(anthropic(500, "") == "Anthropic couldn't answer just now. Try again in a moment.")
    #expect(anthropic(529, #"{"error": {"message": "Overloaded"}}"#) == "Anthropic couldn't answer just now. Try again in a moment.")
    #expect(anthropic(0, "") == "Anthropic can't be reached. Check your internet connection.")
    #expect(anthropic(200, "<html>Sign in to the network</html>") == "Anthropic's answer couldn't be read. Try again in a moment.")
    #expect(anthropic(200, #"{"content": [], "stop_reason": "refusal"}"#) == "Anthropic's safety system turned that down. Ask in other words.")
    #expect(anthropic(400, #"{"error": {"message": "messages: text content blocks must be non-empty"}}"#) == "Anthropic couldn't answer that. Try again in a moment.")
    // A key not taken is said by its status, whatever words come with it.
    #expect(anthropic(401, #"{"error": {"message": "Check your billing details"}}"#) == "Anthropic didn't accept your key. Check it, or replace it.")

    // OpenAI's quota spent is a 429 that waiting does not mend, and a rate limit one that it does;
    // its safety system refuses a picture with a 400. Gemini refuses a key with a 400 that names it,
    // a project's API turned off with a 403, and a picture with a 200 that holds none.
    let quota = #"{"error": {"type": "insufficient_quota", "message": "You exceeded your current quota, please check your plan and billing details."}}"#
    #expect(Why.answered(.openai, status: 429, body: quota) == "Your OpenAI account is out of credit, or past its limit. Check its plan and billing.")
    #expect(Why.answered(.openai, status: 429, body: #"{"error": {"message": "Rate limit reached for requests"}}"#) == "OpenAI is busy. Try again in a moment.")
    let unsafe = #"{"error": {"message": "Your request was rejected by the safety system.", "code": "moderation_blocked"}}"#
    #expect(Why.answered(.openai, status: 400, body: unsafe) == "OpenAI's safety system turned that down. Ask in other words.")
    let gemini = #"{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT"}}"#
    #expect(Why.answered(.gemini, status: 400, body: gemini) == "Gemini didn't accept your key. Check it, or replace it.")
    let disabled = #"{"error": {"code": 403, "message": "Generative Language API has not been used in this project before or it is disabled."}}"#
    #expect(Why.answered(.gemini, status: 403, body: disabled) == "Your Gemini account can't use these models. Check its plan, or choose another model.")
    let blocked = #"{"candidates": [{"finishReason": "IMAGE_SAFETY", "content": {"parts": []}}]}"#
    #expect(Why.answered(.gemini, status: 200, body: blocked) == "Gemini's safety system turned that down. Ask in other words.")

    for status in [0, 200, 400, 401, 402, 403, 404, 408, 413, 429, 500, 503, 529] {
        for body in ["", refusal, credit, quota, unsafe, gemini, disabled] {
            let said = Why.answered(.openai, status: status, body: body)
            #expect(!said.contains(where: \.isNumber), "\(status): \(said)")
            #expect(!said.contains("invalid") && !said.contains("x-api-key") && !said.contains("rejected"), "\(status): \(said)")
        }
    }
}

/// Any error is said in a sentence, without its type's wrapping: stopped; no way to the network,
/// as a person says it, the provider's name where one was asked; an error's own description.
@Test func anErrorIsSaidInASentence() {
    #expect(Why.said(URLError(.notConnectedToInternet), of: .gemini) == "Gemini can't be reached. Check your internet connection.")
    #expect(Why.said(URLError(.timedOut)) == "The network can't be reached. Check your internet connection.")
    #expect(Why.said(URLError(.cancelled)) == "Stopped.")
    #expect(Why.said(CancellationError()) == "Stopped.")

    struct Described: LocalizedError {
        var errorDescription: String? { "the picture is not one the deck can show" }
    }
    #expect(Why.said(Described()) == "The picture is not one the deck can show.")
    struct Unsaid: Error {}
    #expect(Why.said(Unsaid()) == "Something went wrong. Try again.", "an error with no words of its own")
    let file = CocoaError(.fileReadNoSuchFile)
    #expect(!Why.said(file).contains("Error Domain"), "the system says its own: \(Why.said(file))")
}

/// The engine's words are said as they are, which are written for a person: a sentence begun and
/// ended, without the op of the patch they name. Where they name what the deck writes, an id or a
/// key set in backticks, they are said up to it, or not at all.
@Test func theEnginesWordsAreSaidAsTheyAre() throws {
    let engine = { (json: String) in try JSONDecoder().decode(ScaenaError.self, from: Data(json.utf8)) }
    let refused = try engine(#"{"message": "op 2: the deck would not open with it", "op": 2}"#)
    #expect(Why.said(refused) == "The deck would not open with it.")
    let written = try engine(#"{"message": "That picture is larger than a deck takes."}"#)
    #expect(Why.said(written) == "That picture is larger than a deck takes.")
    let numbered = try engine(#"{"message": "op 12 was not applied"}"#)
    #expect(Why.said(numbered) == "Op 12 was not applied.")
    let blank = try engine(#"{"message": " "}"#)
    #expect(Why.said(blank) == "Something went wrong. Try again.")

    // What follows an id the deck writes is no person's to read; with nothing before it, the deck
    // allowed it not.
    let theme = try engine(#"{"message": "not re-themed: role `title` snaps to the baseline grid, and the grid has none"}"#)
    #expect(Why.said(theme) == "Not re-themed.")
    let shown = try engine(#"{"message": "the deck would not open with it. `title` overflows its box"}"#)
    #expect(Why.said(shown) == "The deck would not open with it.")
    let unknown = try engine(#"{"message": "op 0: no state `cover`"}"#)
    #expect(Why.said(unknown) == "The deck doesn't allow it.")
    let named = try engine(#"{"message": "`title` is no text in `cover`"}"#)
    #expect(Why.said(named) == "The deck doesn't allow it.")
}
