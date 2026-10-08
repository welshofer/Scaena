import CScaena
import Foundation

/// A provider of models, with the user's own key (ADR-0006): the browser's three.
public enum Provider: String, Codable, CaseIterable, Identifiable, Sendable {
    case anthropic
    case openai
    case gemini

    public var id: String { rawValue }

    /// Its name, for people.
    public var name: String {
        switch self {
        case .anthropic: "Anthropic"
        case .openai: "OpenAI"
        case .gemini: "Gemini"
        }
    }

    /// The request that lists the models `key` can use.
    public func modelsRequest(key: String, base: String? = nil) throws -> ProviderRequest {
        var args: [String: JSONValue] = ["provider": .string(rawValue), "key": .string(key)]
        if let base { args["base"] = .string(base) }
        return try Self.call("models", .object(args))
    }

    /// The models an answer to `modelsRequest` lists.
    public func models(status: Int, body: String) throws -> [String] {
        try Self.call(
            "readModels",
            [
                "provider": .string(rawValue), "status": .number(Double(status)),
                "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: status)), "body": .string(body),
            ])
    }

    private static func call<T: Decodable>(_ method: String, _ args: JSONValue) throws -> T {
        let json = String(decoding: try JSONEncoder().encode(args), as: UTF8.self)
        return try ScaenaSession.decode(scaena_providers(method, json), as: T.self)
    }
}

/// An HTTP request the engine built for the client to make: what a provider is sent, the user's
/// key in its headers.
public struct ProviderRequest: Decodable, Sendable {
    public let method: String
    public let url: String
    public let headers: [String: String]
    /// The JSON to send; none for a GET.
    public let body: String?

    /// The request as `URLSession` makes it. A model may think for minutes before it answers.
    public func urlRequest() throws -> URLRequest {
        guard let address = URL(string: url) else { throw ScaenaError(message: "\(url) is no address") }
        var request = URLRequest(url: address, timeoutInterval: 600)
        request.httpMethod = method
        for (name, value) in headers {
            request.setValue(value, forHTTPHeaderField: name)
        }
        request.httpBody = body.map { Data($0.utf8) }
        return request
    }
}

/// A call the model made: its id, the tool, and the arguments.
public struct ToolCall: Decodable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let args: JSONValue
}

/// Tokens in and out, as the provider counts them.
public struct Usage: Decodable, Equatable, Sendable {
    public let input: Int
    public let output: Int

    public init(input: Int, output: Int) {
        self.input = input
        self.output = output
    }
}

/// What the model's answer comes to.
public enum Next: Decodable, Sendable {
    /// Calls to run on the bundle, then hand back.
    case calls(text: String, calls: [ToolCall], usage: Usage?)
    /// The answer: the model is done (`end`), ran out of room (`length`), or stopped otherwise.
    case done(text: String, stop: String, usage: Usage?)

    private enum Keys: String, CodingKey { case next, text, calls, stop, usage }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let text = try fields.decode(String.self, forKey: .text)
        let usage = try fields.decodeIfPresent(Usage.self, forKey: .usage)
        if try fields.decode(String.self, forKey: .next) == "calls" {
            self = .calls(text: text, calls: try fields.decode([ToolCall].self, forKey: .calls), usage: usage)
        } else {
            self = .done(text: text, stop: try fields.decode(String.self, forKey: .stop), usage: usage)
        }
    }
}

/// A file a call wrote beside the deck, the theme `theme_edit` edited: its text before and after,
/// none where the bundle did not hold it. An undo writes `before` back.
public struct Rewritten: Codable, Equatable, Sendable {
    public let path: String
    public let before: String?
    public let after: String?

    public init(path: String, before: String?, after: String?) {
        self.path = path
        self.before = before
        self.after = after
    }

    /// The file as an undo of it writes it, and its redo after.
    public var undone: Rewritten { Rewritten(path: path, before: after, after: before) }
}

/// A call run on the bundle: what the model is told, and what the window shows of it.
public struct Ran: Decodable, Sendable {
    public let id: String
    public let name: String
    public let error: Bool
    /// A line saying what it came to.
    public let summary: String
    /// What the model is told: the tool's result, as JSON, or a resource's text.
    public let json: String
    /// The frame `deck_render` drew, a PNG in base64.
    public let png: String?
    /// Whether it changed the deck.
    public let edited: Bool
    /// The files it wrote beside the deck.
    public let rewritten: [Rewritten]

    /// The frame it drew, as PNG bytes.
    public var frame: Data? { png.flatMap { Data(base64Encoded: $0) } }
}

/// What the window shows as a question is asked (PLAN 2.52): the state shown, in a format if not
/// the deck's own, and the nodes selected, the one selected first. "This" in a question means it.
public struct Seeing: Equatable, Sendable {
    /// A node selected, and its type.
    public struct Selected: Equatable, Sendable {
        public var node: String
        public var type: String?

        public init(node: String, type: String? = nil) {
            self.node = node
            self.type = type
        }
    }

    /// Characters selected in a text, from `from` to `to` in Unicode scalar values, as
    /// `replace_text` counts them, and the text they make.
    public struct Characters: Equatable, Sendable {
        public var node: String
        public var from: Int
        public var to: Int
        public var text: String

        public init(node: String, from: Int, to: Int, text: String) {
            self.node = node
            self.from = from
            self.to = to
            self.text = text
        }
    }

    public var state: String
    public var format: String?
    public var nodes: [Selected]
    public var characters: Characters?

    public init(state: String, format: String? = nil, nodes: [Selected] = [], characters: Characters? = nil) {
        self.state = state
        self.format = format
        self.nodes = nodes
        self.characters = characters
    }

    var json: JSONValue {
        var out: [String: JSONValue] = [
            "state": .string(state),
            "nodes": .array(
                nodes.map { selected in
                    var node: [String: JSONValue] = ["node": .string(selected.node)]
                    if let type = selected.type { node["type"] = .string(type) }
                    return .object(node)
                }),
        ]
        if let format { out["format"] = .string(format) }
        if let c = characters {
            out["characters"] = [
                "node": .string(c.node), "from": .number(Double(c.from)), "to": .number(Double(c.to)),
                "text": .string(c.text),
            ]
        }
        return .object(out)
    }
}

/// A conversation with a model (PLAN 3.6, ADR-0022), kept in the engine between questions until
/// it is forgotten. The engine builds each request and reads each answer, as the browser's
/// assistant does; the client makes each request, the user's key handed in for its headers
/// alone, and runs each call the model makes on the session. Use it from the session's thread.
public final class ScaenaChat {
    let handle: OpaquePointer
    public private(set) var provider: Provider
    public private(set) var model: String
    public private(set) var base: String?

    /// A conversation with `model`, of `provider`'s, at `base` where it is not the provider's own.
    public init(provider: Provider, model: String, base: String? = nil) throws {
        var args: [String: JSONValue] = ["provider": .string(provider.rawValue), "model": .string(model)]
        if let base { args["base"] = .string(base) }
        let json = String(decoding: try JSONEncoder().encode(JSONValue.object(args)), as: UTF8.self)
        var error: UnsafeMutablePointer<CChar>?
        guard let made = scaena_chat_new(json, &error) else { throw ScaenaError.taking(error) }
        handle = made
        self.provider = provider
        self.model = model
        self.base = base
    }

    deinit {
        scaena_chat_free(handle)
    }

    /// One step of the conversation, on `session` where it runs anything on it.
    private func step<T: Decodable>(_ method: String, _ args: JSONValue = nil, on session: ScaenaSession? = nil) throws
        -> T
    {
        var json: String?
        if args != .null {
            json = String(decoding: try JSONEncoder().encode(args), as: UTF8.self)
        }
        return try ScaenaSession.decode(scaena_chat_call(handle, session?.handle, method, json), as: T.self)
    }

    /// Ask `text` of the model, begun with what the window shows; the model is told the deck as
    /// `session` holds it now.
    public func ask(_ text: String, seeing: Seeing? = nil, on session: ScaenaSession) throws {
        var args: [String: JSONValue] = ["text": .string(text)]
        if let seeing { args["seeing"] = seeing.json }
        let _: JSONValue = try step("ask", .object(args), on: session)
    }

    /// The request for the model's next turn, `key` in its headers.
    public func request(key: String) throws -> ProviderRequest {
        try step("request", ["key": .string(key)])
    }

    /// Read the answer to `request`: what comes next. A provider's refusal is thrown, and the
    /// conversation is as it was.
    public func answer(status: Int, body: String) throws -> Next {
        try step(
            "answer",
            [
                "status": .number(Double(status)),
                "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: status)), "body": .string(body),
            ])
    }

    /// Run the last answer's call `index` on `session`, by `agent:` and the model's name, at `date`.
    public func run(_ index: Int, on session: ScaenaSession, at date: Date = Date()) throws -> Ran {
        try step("run", ["index": .number(Double(index)), "at": .string(date.formatted(.iso8601))], on: session)
    }

    /// Hand the calls' results to the model, every call answered, those not run as stopped:
    /// whether it may be asked again, or the question has taken its rounds.
    public func next() throws -> Bool {
        try step("next")
    }

    /// Another model from the next request on: the conversation goes on.
    public func use(provider: Provider, model: String, base: String? = nil) throws {
        var args: [String: JSONValue] = ["provider": .string(provider.rawValue), "model": .string(model)]
        if let base { args["base"] = .string(base) }
        let _: JSONValue = try step("use", .object(args))
        self.provider = provider
        self.model = model
        self.base = base
    }

    /// Start again: the next question is the first.
    public func forget() {
        let _: JSONValue? = try? step("forget")
    }

    /// The conversation as the engine keeps it, in no provider's form.
    public func conversation() throws -> JSONValue {
        try step("conversation")
    }
}
