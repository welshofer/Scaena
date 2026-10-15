import CoreGraphics
import Foundation

/// What a haiku is about (PLAN O.4g, ADR-0030): a topic typed, the slide shown, or the whole talk.
public enum HaikuAbout: String, Codable, CaseIterable, Sendable {
    case topic, slide, talk
}

/// A haiku asked of the deck as it stands, from the state shown; and, for Write Another, the poem
/// not to write again.
public struct HaikuAsk: Sendable, Equatable {
    public var state: String
    public var about: HaikuAbout
    public var topic: String?
    public var another: [String]?

    public init(state: String, about: HaikuAbout, topic: String? = nil, another: [String]? = nil) {
        self.state = state
        self.about = about
        self.topic = topic
        self.another = another
    }

    /// As the session takes it.
    var json: JSONValue {
        var out: [String: JSONValue] = ["state": .string(state), "about": .string(about.rawValue)]
        if let topic { out["topic"] = .string(topic) }
        if let another { out["another"] = .array(another.map { .string($0) }) }
        return .object(out)
    }
}

/// What to ask the model for a haiku (`scaena_core::writing::haiku`): its instructions and
/// prompt, in the deck's language, and what it is about as its slide's notes will say it.
public struct HaikuWriting: Decodable, Sendable {
    public let instructions: String
    public let prompt: String
    public let lang: String
    public let about: String
}

/// A line's count: the fewest and the most syllables, or morae, it may be said in.
public struct VerseCount: Codable, Sendable, Equatable {
    public let low: Int
    public let high: Int

    /// As a person says it: `5`, or `6 or 7`.
    public var said: String { low == high ? "\(low)" : "\(low) or \(high)" }
}

/// A poem counted against 5-7-5: each line, its count (none where it is not counted) and the
/// count it keeps to, and how the whole keeps to them.
public struct VerseChecked: Codable, Sendable {
    public struct Line: Codable, Sendable {
        public let text: String
        public let count: VerseCount?
        public let target: Int?

        /// Off its count: a line that cannot be counted is not.
        public var off: Bool {
            guard let count, let target else { return false }
            return target < count.low || target > count.high
        }
    }

    public let lines: [Line]
    /// `kept`, `near`, `off`, or `uncounted`.
    public let kept: String
}

/// A haiku the engine counted: its lines, their reading where a language writes one, and how they
/// keep to 5-7-5.
public struct Poem: Codable, Sendable {
    public let lines: [String]
    public let reading: [String]?
    public let checked: VerseChecked

    /// As the session takes it back.
    var json: JSONValue {
        get throws { try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(self)) }
    }
}

/// The patch that places a haiku, the state it is on, and its text.
public struct HaikuPlaced: Decodable, Sendable {
    public let patch: [JSONValue]
    public let state: String
    public let node: String
}

/// What an answer comes to: the poem to offer, with a note where it is off 5-7-5 and the patch that
/// places it where it was asked to go; or the second ask, with the poem the first came to.
public enum HaikuTaken: Decodable, Sendable {
    case offer(poem: Poem, note: String?, placed: HaikuPlaced?)
    case again(prompt: String, poem: Poem)

    private enum Keys: String, CodingKey {
        case next, poem, note, placed, prompt
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let poem = try c.decode(Poem.self, forKey: .poem)
        switch try c.decode(String.self, forKey: .next) {
        case "again":
            self = .again(prompt: try c.decode(String.self, forKey: .prompt), poem: poem)
        default:
            self = .offer(
                poem: poem, note: try c.decodeIfPresent(String.self, forKey: .note),
                placed: try c.decodeIfPresent(HaikuPlaced.self, forKey: .placed))
        }
    }

    /// The poem it comes to.
    public var poem: Poem {
        switch self {
        case .offer(let poem, _, _), .again(_, let poem): poem
        }
    }
}

/// Where a haiku taken goes: a new Haiku slide after the slide shown, or this slide (its empty
/// verse place, the verse given, else where the pointer last pressed).
public enum HaikuPlace: String, Sendable {
    case new, slide
}

/// Who wrote a haiku, as its slide's notes say it: the model's name, and the provider whose key
/// reached it; none for the model on the device.
public struct HaikuWriter: Sendable, Equatable {
    public var model: String
    public var provider: String?

    public init(model: String, provider: String? = nil) {
        self.model = model
        self.provider = provider
    }
}

/// Where a haiku goes on the slide shown, as the canvas finds it (PLAN O.4g): over the verse
/// selected, alone; in the slide's empty verse place; else where the pointer last pressed, or on a
/// new slide after this one, which the sheet asks.
public struct HaikuTarget: Identifiable, Sendable {
    public let id = UUID()
    public let state: String
    /// The verse selected, which the poem replaces.
    public let node: String?
    /// Whether the slide has an empty place for a verse, which the poem fills.
    public let waiting: Bool
    /// Where the pointer last pressed, canvas units.
    public let at: CGPoint?

    /// Whether the sheet asks where the poem goes: not where a verse is selected or waits.
    public var asksWhere: Bool { node == nil && !waiting }
}

extension ScaenaSession {
    /// What to ask a model for `ask` (PLAN O.4d), in the deck's language: refused, saying why,
    /// where the theme's fonts cannot draw it or a topic is empty.
    public func haiku(_ ask: HaikuAsk) throws -> HaikuWriting { try call("haiku", ask.json) }

    /// What `answer` comes to for `ask`: the poem to offer, or, off 5-7-5 the first time, the
    /// second ask; `first`, the poem the first answer came to, with the second answer.
    public func haikuTaken(_ ask: HaikuAsk, answer: String, first: Poem? = nil) throws -> HaikuTaken {
        var args: [String: JSONValue] = ["ask": ask.json, "answer": .string(answer)]
        if let first { args["first"] = try first.json }
        return try call("haikuTaken", .object(args))
    }

    /// The poem `lines` as the person left them, counted, and the patch that places it as `to` says
    /// from `ask`'s state: over `node`, a verse given, or at `at` where the slide has no place
    /// for one; the notes naming `by` and `date`, and `attribution` under it only where given.
    public func haikuTaken(
        _ ask: HaikuAsk, lines: [String], to: HaikuPlace, node: String? = nil, at: CGPoint? = nil,
        attribution: String? = nil, by: HaikuWriter, date: String
    ) throws -> HaikuTaken {
        var writer: [String: JSONValue] = ["model": .string(by.model)]
        if let provider = by.provider { writer["provider"] = .string(provider) }
        var args: [String: JSONValue] = [
            "ask": ask.json, "lines": .array(lines.map { .string($0) }), "to": .string(to.rawValue),
            "by": .object(writer), "date": .string(date),
        ]
        if let node { args["node"] = .string(node) }
        if let at { args["at"] = .array([.number(Double(at.x)), .number(Double(at.y))]) }
        if let attribution, !attribution.isEmpty { args["attribution"] = .string(attribution) }
        return try call("haikuTaken", .object(args))
    }

    /// Where a haiku asked on `state` goes, `selected` selected and the pointer last pressed at
    /// `pressed`, as the browser's canvas finds it.
    public func haikuTarget(state: String, selected: [String], pressed: CGPoint?) throws -> HaikuTarget {
        var verse: String?
        if selected.count == 1, let one = selected.first {
            let choices = try self.choices(state: state, node: one)
            let value = { (prop: String) in choices.fields.first { $0.prop == prop }?.value?.string }
            if choices.type == "text", value("role") == "verse" || value("wrap") == "none" { verse = one }
        }
        let waiting = try self.waiting(state: state).contains { $0.typed && $0.role == "verse" }
        return HaikuTarget(state: state, node: verse, waiting: waiting, at: pressed)
    }

    /// Each line's count as the deck's language counts it, as the sheet shows it while a line is
    /// edited.
    public func verseCounts(_ lines: [String]) throws -> VerseChecked {
        try call("verseCounts", ["lines": .array(lines.map { .string($0) })])
    }
}

/// A haiku written for the person editing (PLAN O.4g, ADR-0030 decisions 26–29), as the browser's
/// dialog writes one: the engine says what to ask, the model answers, and an answer off 5-7-5 is
/// asked once more with each line's count. `respond` asks the model, told the instructions, the
/// prompt: the assistant's with the user's key, or the one on the device.
public enum Haiku {
    /// What the model wrote for `ask`, counted: the second answer where the first was off, with a
    /// note where the poem offered still is.
    @MainActor
    public static func written(
        _ ask: HaikuAsk, of session: ScaenaSession,
        respond: (_ instructions: String, _ prompt: String) async throws -> String
    ) async throws -> (poem: Poem, note: String?) {
        let writing = try session.haiku(ask)
        let first = try session.haikuTaken(ask, answer: try await respond(writing.instructions, writing.prompt))
        switch first {
        case .offer(let poem, let note, _):
            return (poem, note)
        case .again(let prompt, let poem):
            let second = try await respond(writing.instructions, prompt)
            switch try session.haikuTaken(ask, answer: second, first: poem) {
            case .offer(let poem, let note, _): return (poem, note)
            case .again(_, let poem): return (poem, nil)
            }
        }
    }

    /// Today as a haiku's notes say it: `October 15, 2026`.
    public static func today(_ now: Date = .now) -> String {
        let format = DateFormatter()
        format.locale = Locale(identifier: "en_US_POSIX")
        format.dateFormat = "MMMM d, yyyy"
        return format.string(from: now)
    }
}
