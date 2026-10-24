import Foundation

/// What a person asks a model to write (PLAN 2.101, `scaena_core::writing`): a text's words
/// proofread, tightened, or rewritten; a slide's notes drafted; an issue explained; a picture
/// described from itself (PLAN 2.119); a slide's words checked against its beat's claim, its
/// headline rewritten to carry it (narrative FUNC-4).
public enum WritingKind: String, Codable, CaseIterable, Sendable {
    case proofread, tighten, rewrite, notes, explain, describe, claim
}

/// A writing tool asked of the deck as it stands: the task, the state shown, and the text it is
/// about, or the finding to explain.
public struct WritingAsk: Sendable {
    public var kind: WritingKind
    public var state: String
    public var node: String?
    public var finding: Finding?

    public init(kind: WritingKind, state: String, node: String? = nil, finding: Finding? = nil) {
        self.kind = kind
        self.state = state
        self.node = node
        self.finding = finding
    }

    /// As the session takes it.
    var json: JSONValue {
        var out: [String: JSONValue] = ["kind": .string(kind.rawValue), "state": .string(state)]
        if let node { out["node"] = .string(node) }
        if let finding {
            var f: [String: JSONValue] = [
                "code": .string(finding.code), "severity": .string(finding.severity.rawValue),
                "message": .string(finding.message),
            ]
            // Asked as the window says it, as the browser asks: no node's id reaches the model.
            if let said = finding.said { f["said"] = .string(said) }
            if let hint = finding.hint { f["hint"] = .string(hint) }
            if let node = finding.node { f["node"] = .string(node) }
            out["finding"] = .object(f)
        }
        return .object(out)
    }
}

/// What a model is asked for a writing tool: what it is called where it is offered, and the
/// instructions and the prompt it answers; with the words it works on, where it works on a text's,
/// or a picture's description; and, to describe a picture, the picture as the slide shows it, a
/// PNG in base64, which the model is shown (PLAN 2.119).
public struct Writing: Decodable, Sendable {
    public struct Task: Decodable, Sendable {
        public let kind: WritingKind
        public let text: String?
    }

    public let task: Task
    public let title: String
    public let instructions: String
    public let prompt: String
    public let picture: String?
}

/// An answer as it is offered, and the patch that takes it: none for one that is only read.
public struct Written: Decodable, Sendable {
    public let text: String
    public let patch: [JSONValue]?
}

extension ScaenaSession {
    /// What to ask a model for `ask`, from the deck as it stands (PLAN 2.101): a text's words as
    /// the state shows them, the slide's parts as they read and the claim of its beat, or the
    /// finding with the words of what it is about.
    public func writing(_ ask: WritingAsk) throws -> Writing { try call("writing", ask.json) }

    /// `answer` as it is offered for `ask`, and the patch that takes it, made on the deck as it
    /// stands: the `replace_text`s that make the text read so, the words it keeps keeping their
    /// runs, or the state's notes. One patch, one step to undo.
    public func written(_ ask: WritingAsk, answer: String) throws -> Written {
        try call("written", ["ask": ask.json, "answer": .string(answer)])
    }

    /// The patch Use it makes of `answer` for `ask`: none where it changes nothing, the text or
    /// the notes already reading so, or the model gave no answer; an error where it cannot be
    /// taken, which says why.
    public func taking(_ ask: WritingAsk, answer: String) throws -> [JSONValue]? {
        let ops = try written(ask, answer: answer).patch ?? []
        return ops.isEmpty ? nil : ops
    }
}

/// Why the model on the device gave a writing tool no answer where the user's own model may give
/// one (lift-up STAB-4): more than it takes at once, its guardrails, a language it does not write,
/// or its model not on the device yet. Any other failure is no reason to ask elsewhere.
public struct Declined: LocalizedError, Sendable, Equatable {
    public enum Reason: Sendable, Equatable {
        case tooLong, guardrails, language, notReady
    }

    public let reason: Reason

    public init(_ reason: Reason) {
        self.reason = reason
    }

    /// It, in a person's words (PLAN 3.18), as the sheet notes it.
    public var said: String {
        switch reason {
        case .tooLong: "Apple Intelligence can't take this much at once"
        case .guardrails: "Apple Intelligence declined this"
        case .language: "Apple Intelligence doesn't write this language"
        case .notReady: "Apple Intelligence isn't ready on this device yet"
        }
    }

    public var errorDescription: String? { "\(said)." }
}

/// Who answers a writing tool where the model on the device is asked first (lift-up STAB-4): its
/// answer, or, where it declines (`Declined`), the decline, with the user's own model to ask where
/// a key is kept. Nothing is asked of that model here: the words leave the device only when the
/// person agrees (`Elsewhere.agreed`). Any other failure is thrown as it came.
public enum Answering {
    public enum Outcome: Sendable {
        case answered(String)
        /// The device declined; `elsewhere` is the user's own model, none where no key is kept.
        case declined(Declined, elsewhere: Elsewhere?)
    }

    /// The user's own model, which a decline offers to ask with their key.
    public struct Elsewhere: Sendable {
        public let model: String
        /// The provider, by its name.
        public let provider: String
        let ask: @Sendable () async throws -> String

        public init(model: String, provider: String, ask: @escaping @Sendable () async throws -> String) {
            self.model = model
            self.provider = provider
            self.ask = ask
        }

        /// The button that asks it, in a person's words (PLAN 3.18).
        public var offered: String { "Ask \(model) with your \(provider) key" }

        /// Where its answer came from, as the sheet notes it.
        public var whence: String { "Written by \(model), with your \(provider) key, as you asked." }

        /// Its answer, asked once the person has agreed to send the words off the device.
        public func agreed() async throws -> String { try await ask() }
    }

    public static func answer(
        _ onDevice: @Sendable () async throws -> String,
        else elsewhere: Elsewhere?
    ) async throws -> Outcome {
        do {
            return .answered(try await onDevice())
        } catch let declined as Declined {
            return .declined(declined, elsewhere: elsewhere)
        }
    }
}
