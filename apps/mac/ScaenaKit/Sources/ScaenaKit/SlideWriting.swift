import Foundation

/// Where Write This Slide Again… or Add a Slide About… writes a slide (lift-up FUNC-1): in the
/// place of the slide whose first state it names, or after it.
public enum SlidePlace: Equatable, Sendable {
    case replace(String)
    case after(String)

    var json: JSONValue {
        switch self {
        case .replace(let state): ["replace": .string(state)]
        case .after(let state): ["after": .string(state)]
        }
    }
}

/// What a model is asked for one slide: Narrato's one-slide instructions, and the prompt with the
/// deck's brief, arc, part of the talk, and the slides either side fenced as data, the bytes the
/// browser's generator module sends (`scaena_ops::narrative::one_request`).
public struct SlideAsked: Decodable, Equatable, Sendable {
    public let system: String
    public let prompt: String
}

/// A slide written into the deck: the source it replaced, for one step of undo; the state that
/// begins it, to show; and how many issues lint still finds on it.
public struct SlideWritten: Sendable {
    public let before: String
    public let first: String?
    public let left: Int
}

extension ScaenaSession {
    /// What to ask a model for one slide at `place`, `ask` the person's words.
    public func slideAsk(_ place: SlidePlace, ask: String) throws -> SlideAsked {
        try call("slideAsk", ["place": place.json, "ask": .string(ask)])
    }

    /// The slide a model's `answer` to `slideAsk` holds, as `slide_write` takes it; or why it is
    /// none, thrown.
    public func slideRead(_ answer: String) throws -> JSONValue {
        try call("slideRead", ["answer": .string(answer)])
    }
}

extension DeckEditor {
    /// One slide written at `place` by `model` of `provider`'s with the user's `key` (lift-up
    /// FUNC-1), as the browser's Write This Slide Again… and Add a Slide About… write one: asked
    /// once with nothing to call, its answer read as Narrato writes a slide, and the slide written
    /// as `slide_write` writes one, by `agent:` and the model's name, one patch. An answer cut off
    /// or declined, one that holds no slide, or a deck the slide would not open in changes
    /// nothing, and says why; so does the assistant at work on the deck, before anything is sent
    /// and once the answer comes. The caller makes `before` one step of undo, as it takes the
    /// assistant's edits.
    @MainActor
    public func writeSlide(
        _ place: SlidePlace, ask: String, provider: Provider, model: String, key: String, base: String? = nil,
        send: Assistant.Send = Assistant.urlSession
    ) async throws -> SlideWritten {
        try unassisted()
        let asked = try session.slideAsk(place, ask: ask)
        let answer = try await provider.write(
            model: model, key: key, base: base, instructions: asked.system, prompt: asked.prompt, send: send)
        try Task.checkCancellation()
        try unassisted()
        let slide = try session.slideRead(answer)
        var args: [String: JSONValue] = ["slide": slide]
        switch place {
        case .replace(let state): args["replace"] = .string(state)
        case .after(let state): args["after"] = .string(state)
        }
        let called = try session.tool("slide_write", .object(args), author: "agent:\(model)")
        guard called.edited, let before = reread() else {
            throw ScaenaError(message: "the deck would not take the slide")
        }
        return SlideWritten(
            before: before, first: called.result["slide"]?[0]?.string, left: called.result["left"]?.array?.count ?? 0)
    }
}
