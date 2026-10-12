import CScaena
import Foundation

/// What a brief may ask, as Narrato knows it (PLAN G.6): its goals, lengths, and ways a deck is
/// delivered, the audiences its personas name, its arcs, and the defaults a field left empty takes.
public struct BriefChoices: Decodable, Sendable {
    public struct Modality: Decodable, Sendable, Hashable {
        public let id: String
        public let label: String
    }

    public struct Defaults: Decodable, Sendable {
        public let audience: String
        public let goal: String
        public let length: String
    }

    public let goals: [String]
    public let lengths: [String]
    public let modalities: [Modality]
    public let audiences: [String]
    public let arcs: [String]
    public let defaults: Defaults
}

/// Narrato's brief (`scaena_narrato::Brief`): what the deck is about, in the user's words, and what
/// they say of it; a field left empty takes Narrato's default.
public struct Brief: Encodable, Sendable {
    public var topic: String
    public var audience: String?
    public var goal: String?
    public var length: String?
    public var modality: String?
    public var desiredOutcomes: String?
    public var language: String?
    public var voice: String?

    public init(
        topic: String, audience: String? = nil, goal: String? = nil, length: String? = nil, modality: String? = nil,
        desiredOutcomes: String? = nil, language: String? = nil, voice: String? = nil
    ) {
        self.topic = topic
        self.audience = audience
        self.goal = goal
        self.length = length
        self.modality = modality
        self.desiredOutcomes = desiredOutcomes
        self.language = language
        self.voice = voice
    }
}

/// How far a deck being made from a prompt has come, as `scaena generate` says it.
public enum GeneratingStep: Equatable, Sendable {
    case fetchingFont(family: String, language: String, megabytes: Double)
    case reading(String)
    case read(name: String, kind: String)
    case researching(String)
    case researchingNow(String)
    case researched(sources: Int, cut: Bool)
    case choosing
    case chose(arc: String, fallback: String?)
    case writing(slides: Int)
    case wrote(slides: Int, truncated: Bool)
    case painting(pictures: Int, model: String)
    case painted(pictures: Int, unmade: [String])
    case making
    case looking(slides: Int)
    case looked(slides: Int)
    case unlooked(String)
    case fitting(texts: Int)
    case fitted(texts: Int)
    case unfitted(String)

    /// What it says, in a person's words.
    public var words: String {
        switch self {
        case .fetchingFont(let family, let language, let megabytes):
            "Fetching \(family), the font a deck in \(language) is set in (\(String(format: "%.1f", megabytes)) MB)…"
        case .reading(let name): "Reading \(name)…"
        case .read(let name, let kind): "Read \(name) (\(kind))"
        case .researching(let topic): "Researching \(topic)… (minutes)"
        case .researchingNow(let doing): doing
        case .researched(let sources, let cut): "Researched: \(sources) sources\(cut ? ", cut short" : "")"
        case .choosing: "Choosing the arc…"
        case .chose(let arc, let fallback):
            fallback.map { "Arc: \(arc), the heuristic's (\($0))" } ?? "Arc: \(arc)"
        case .writing(let slides): "Writing the slides, at least \(slides)…"
        case .wrote(let slides, let truncated): "\(slides) slides written\(truncated ? ", the answer cut short" : "")"
        case .painting(let pictures, let model): "Making \(pictures) pictures with \(model)… (a minute or two)"
        case .painted(let pictures, let unmade) where unmade.isEmpty: "\(pictures) pictures made"
        case .painted(let pictures, let unmade): "\(pictures) pictures made; not made: \(unmade.joined(separator: "; "))"
        case .making: "Making the deck in the theme…"
        case .looking(let slides): "Showing the model the \(slides) slides as drawn…"
        case .looked(let slides) where slides == 0: "The model saw nothing to change"
        case .looked(let slides): "\(slides) \(slides == 1 ? "slide" : "slides") changed by what the model saw"
        case .unlooked(let why): "Not changed by what the model saw: \(why)"
        case .fitting(let texts): "\(texts) \(texts == 1 ? "text runs" : "texts run") past their place: asking for them shorter…"
        case .fitted(let texts): "\(texts) rewritten to fit"
        case .unfitted(let why): "Not rewritten to fit: \(why)"
        }
    }
}

/// What a deck made from a prompt came to, as `scaena generate --json` says it.
public struct Generated: Decodable, Sendable {
    public struct Created: Decodable, Sendable {
        public let created: Bool
        public let errors: Int
    }

    public let created: Created
    public let arc: String
    public let fallback: String?
    public let layouts: [String]
    public let fitted: [String]?
    public let unfitted: String?
    /// How many pictures were made for its slides, and why each other asked for was not.
    public let pictures: Int?
    public let unpictured: [String]?
    /// What the model changed on the deck it saw drawn (PLAN G.13), slide by slide, or why not.
    public let looked: [Looked]?
    public let unlooked: String?

    /// A slide the model changed: its number, the layout it gave it, and why.
    public struct Looked: Decodable, Sendable {
        public let slide: Int
        public let layout: String?
        public let why: String
    }

    /// What was made, in a line.
    public var words: String {
        let errors = created.errors == 0 ? "" : ", \(created.errors) \(created.errors == 1 ? "error" : "errors") to look at"
        let made = pictures.map { $0 == 0 ? "" : ", \($0) \($0 == 1 ? "picture" : "pictures")" } ?? ""
        let seen = (looked?.count).map { $0 == 0 ? "" : ", \($0) changed by what the model saw" } ?? ""
        return "Made \(layouts.count) slides on the arc \(arc)\(made)\(seen)\(errors)"
    }
}

/// A request a step asks the client to make, and how much of its answer to read: a GET of a link's
/// plan, or a provider's call.
public struct GeneratorRequest: Decodable, Sendable {
    public let method: String
    public let url: String
    public let headers: [String: String]
    public let body: String?
    public let most: Int?

    /// The request as `URLSession` makes it. A model may think for minutes before it answers.
    public func urlRequest() throws -> URLRequest {
        guard let address = URL(string: url) else { throw ScaenaError(message: "\(url) is no address") }
        var request = URLRequest(url: address, timeoutInterval: 900)
        request.httpMethod = method
        for (name, value) in headers {
            request.setValue(value, forHTTPHeaderField: name)
        }
        request.httpBody = body.map { Data($0.utf8) }
        return request
    }
}

/// An answer to a request: its status, its content type, its body, and its `Retry-After`.
public struct GeneratorReply: Sendable {
    public let status: Int
    public let kind: String
    public let body: Data
    public let after: String?

    public init(status: Int, kind: String = "", body: Data, after: String? = nil) {
        self.status = status
        self.kind = kind
        self.body = body
        self.after = after
    }

    /// `request` made by `URLSession`: status 0 where it never came back, saying why.
    public static func made(_ request: GeneratorRequest, by session: URLSession = .shared) async -> GeneratorReply {
        do {
            let (data, response) = try await session.data(for: try request.urlRequest())
            let http = response as? HTTPURLResponse
            return GeneratorReply(
                status: http?.statusCode ?? 0, kind: http?.value(forHTTPHeaderField: "content-type") ?? "", body: data,
                after: http?.value(forHTTPHeaderField: "retry-after"))
        } catch {
            return GeneratorReply(status: 0, kind: "", body: Data(error.localizedDescription.utf8))
        }
    }

    var text: String { String(decoding: body, as: UTF8.self) }
}

/// A deck made from a prompt (PLAN G.6, ADR-0026), as `scaena generate` makes one: the engine reads
/// each source, plans each link's GETs, builds each request and reads each answer, and makes the
/// deck a bundle; the client makes each request, the user's key handed in for its headers alone.
/// `send` makes a request: `URLSession` in the app, recorded answers in the tests.
@MainActor
public final class Generator {
    /// How a request is made, and what it answered.
    public typealias Send = @Sendable (GeneratorRequest) async -> GeneratorReply

    private let made = Made()
    private var handle: OpaquePointer { made.pointer }
    private let send: Send
    /// Where a script's font is kept once fetched, by its digest (PLAN G.22).
    private let fonts: URL?
    /// The script whose font the engine was given, so that it is not handed over twice.
    private var scripted: String?

    public init(send: @escaping Send = { await GeneratorReply.made($0) }, fonts: URL? = Generator.fontsKept) {
        self.send = send
        self.fonts = fonts
    }

    /// Where this app keeps the scripts' fonts it fetched: Application Support.
    nonisolated public static var fontsKept: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appending(path: "Scaena/Fonts")
    }

    /// The engine's generation, freed with it.
    private final class Made {
        let pointer = scaena_generator_new()!

        deinit {
            scaena_generator_free(pointer)
        }
    }

    /// What a brief may ask.
    nonisolated public static func choices() throws -> BriefChoices {
        try ScaenaSession.decode(scaena_generator_choices(), as: BriefChoices.self)
    }

    private func step<T: Decodable>(_ method: String, _ args: [String: JSONValue] = [:], bytes: Data? = nil) throws -> T {
        let json = String(decoding: try JSONEncoder().encode(JSONValue.object(args)), as: UTF8.self)
        guard let bytes else {
            return try ScaenaSession.decode(scaena_generator_call(handle, method, json, nil, 0), as: T.self)
        }
        return try bytes.withUnsafeBytes { raw in
            try ScaenaSession.decode(
                scaena_generator_call(handle, method, json, raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count),
                as: T.self)
        }
    }

    private struct Read: Decodable {
        let name: String
        let kind: String
    }

    private struct ScriptAsked: Decodable {
        struct Script: Decodable {
            let key: String
            let family: String
            let language: String
            let url: String
            let sha256: String
            let bytes: Int
        }

        let script: Script?
    }

    /// Ready to write a deck in `language` in the theme that ships as `theme`, before a file is
    /// read or any call is paid for (PLAN G.21, G.22): where the theme's fonts do not draw the
    /// language's letters, the script's font kept here, else fetched where the engine says, is
    /// handed to the engine, which holds it to its pinned digest; then the theme is asked whether
    /// its fonts draw the language, and a deck its fonts cannot draw is refused, saying why.
    public func prepare(language: String?, theme: String, hear: (GeneratingStep) -> Void = { _ in }) async throws {
        var args: [String: JSONValue] = ["theme": .string(theme)]
        if let language { args["language"] = .string(language) }
        let asked: ScriptAsked = try step("script", args)
        if let script = asked.script, script.key != scripted {
            try await give(script, hear: hear)
            scripted = script.key
        }
        let _: JSONValue = try step("writable", args)
    }

    /// `script`'s font handed to the engine: the one kept here where it is still the font pinned,
    /// else fetched, then kept.
    private func give(_ script: ScriptAsked.Script, hear: (GeneratingStep) -> Void) async throws {
        let keyed: [String: JSONValue] = ["key": .string(script.key)]
        let kept = fonts?.appending(path: script.sha256)
        if let kept, let data = try? Data(contentsOf: kept) {
            do {
                let _: JSONValue = try step("scriptFont", keyed, bytes: data)
                return
            } catch {
                // Not the font pinned: fetched again below.
            }
        }
        hear(.fetchingFont(family: script.family, language: script.language, megabytes: Double(script.bytes) / 1e6))
        let reply = await send(GeneratorRequest(method: "GET", url: script.url, headers: [:], body: nil, most: nil))
        guard reply.status == 200 else {
            let why = reply.status == 0 ? reply.text : HTTPURLResponse.localizedString(forStatusCode: reply.status)
            throw ScaenaError(
                message: "a deck in \(script.language) is set in \(script.family), which could not be fetched from \(script.url): \(why)")
        }
        let _: JSONValue = try step("scriptFont", keyed, bytes: reply.body)
        if let kept {
            // A font that cannot be kept is fetched again for the next deck.
            try? FileManager.default.createDirectory(at: kept.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? reply.body.write(to: kept, options: .atomic)
        }
    }

    /// The file `name`, whose bytes are `data`, read by its extension: Markdown, text, RTF, HTML,
    /// JSON, CSV, Excel, Word, PowerPoint as words alone, or a PDF, which the model reads as itself.
    @discardableResult
    public func add(name: String, data: Data) throws -> GeneratingStep {
        let read: Read = try step("add", ["name": .string(name)], bytes: data)
        return .read(name: read.name, kind: read.kind)
    }

    private struct Fetched: Decodable {
        let get: GeneratorRequest?
        let read: Read?
    }

    /// What `link` names, fetched a GET at a time: `sec:TICKER` with the user's `contact`, a GitHub
    /// repository with their `token` where it is private, a Google file shared by link, or a page.
    @discardableResult
    public func fetch(link: String, contact: String? = nil, token: String? = nil) async throws -> GeneratingStep {
        var args: [String: JSONValue] = ["spec": .string(link)]
        if let contact { args["contact"] = .string(contact) }
        if let token { args["token"] = .string(token) }
        var next: Fetched = try step("fetch", args)
        while let get = next.get {
            let reply = await send(get)
            if reply.status == 0 { throw ScaenaError(message: "\(get.url) could not be reached: \(reply.text)") }
            next = try step("fetched", ["status": .number(Double(reply.status)), "kind": .string(reply.kind)], bytes: reply.body)
        }
        guard let read = next.read else { throw ScaenaError(message: "\(link) read nothing") }
        return .read(name: read.name, kind: read.kind)
    }

    private struct Researching: Decodable {
        struct Done: Decodable {
            let sources: Int
            let cut: Bool
        }

        let send: GeneratorRequest?
        let wait: Int?
        let done: Done?
        let progress: String?
    }

    /// Deep research on the brief's topic (PLAN G.8), its report kept as a source: Gemini's agent
    /// polled, or Anthropic's search resumed, each wait kept here.
    public func research(
        brief: Brief, provider: Provider, model: String, base: String? = nil, key: String,
        hear: (GeneratingStep) -> Void
    ) async throws {
        var args = try asking(brief: brief, provider: provider, model: model, base: base)
        args["key"] = .string(key)
        hear(.researching(brief.topic))
        var next: Researching = try step("research", args)
        var doing: String?
        while let request = next.send {
            if let wait = next.wait { try await Task.sleep(nanoseconds: UInt64(wait) * 1_000_000) }
            let reply = await send(request)
            next = try step(
                "researched",
                [
                    "key": .string(key), "status": .number(Double(reply.status)),
                    "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                    "body": .string(reply.text),
                ])
            if let progress = next.progress, progress != doing {
                doing = progress
                hear(.researchingNow(progress))
            }
        }
        if let done = next.done { hear(.researched(sources: done.sources, cut: done.cut)) }
    }

    private func asking(brief: Brief, provider: Provider, model: String, base: String?) throws -> [String: JSONValue] {
        let briefed = try JSONDecoder().decode(JSONValue.self, from: try JSONEncoder().encode(brief))
        var args: [String: JSONValue] = [
            "brief": briefed, "provider": .string(provider.rawValue), "model": .string(model),
        ]
        if let base { args["base"] = .string(base) }
        return args
    }

    private struct Standing: Decodable {
        let step: String
        let slides: Int?
        let request: GeneratorRequest?
        let arc: String?
        let fallback: String?
        let written: Int?
        let truncated: Bool?
    }

    private struct Making: Decodable {
        struct Look: Decodable {
            let request: GeneratorRequest
            let slides: Int
        }

        let look: Look?
        let fit: Fit?
    }

    private struct Fit: Decodable {
        let request: GeneratorRequest
        let texts: Int
    }

    private struct Looking: Decodable {
        let looked: Int
        let unlooked: String?
        let fit: Fit?
    }

    private struct Fitting: Decodable {
        let fitted: Int?
        let unfitted: String?
    }

    /// Who makes a deck's pictures (PLAN G.10): an image provider, the user's key for it, where
    /// its API is where not its own, and how they look (`illustration` or `photo`).
    public struct Painter: Sendable {
        public let provider: Provider
        public let key: String
        public let base: String?
        public let look: String

        public init(provider: Provider, key: String, base: String? = nil, look: String = "illustration") {
            self.provider = provider
            self.key = key
            self.base = base
            self.look = look
        }
    }

    private struct Listing: Decodable {
        let request: GeneratorRequest
    }

    private struct Models: Decodable {
        let models: [String]
    }

    private struct Asked: Decodable {
        let pictures: Int
    }

    private struct Round: Decodable {
        let requests: [GeneratorRequest]
    }

    private struct Wait: Decodable {
        let wait: Double
    }

    private struct Finished: Decodable {
        let pictures: Int
        let unmade: [String]
    }

    /// The pictures the narrative's slides ask for, made by `painter`'s best image model, every
    /// one at once, a busy provider asked again after the wait the engine says; kept for `make`.
    private func paint(_ painter: Painter, theme: String, hear: (GeneratingStep) -> Void) async throws {
        var listing: [String: JSONValue] = ["provider": .string(painter.provider.rawValue), "key": .string(painter.key)]
        if let base = painter.base { listing["base"] = .string(base) }
        let list: Listing = try step("imageModels", listing)
        let listed = await send(list.request)
        let models: Models = try step(
            "imageModelsRead",
            [
                "provider": .string(painter.provider.rawValue), "status": .number(Double(listed.status)),
                "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: listed.status)),
                "body": .string(listed.text),
            ])
        guard let model = models.models.first else {
            hear(.painted(pictures: 0, unmade: ["your \(painter.provider.name) key lists no image model"]))
            return
        }
        var asking: [String: JSONValue] = [
            "theme": .string(theme), "look": .string(painter.look), "provider": .string(painter.provider.rawValue),
            "model": .string(model),
        ]
        if let base = painter.base { asking["base"] = .string(base) }
        let asked: Asked = try step("pictures", asking)
        guard asked.pictures > 0 else { return }
        hear(.painting(pictures: asked.pictures, model: model))
        while true {
            let round: Round = try step("paint", ["key": .string(painter.key)])
            if round.requests.isEmpty { break }
            let send = self.send
            let replies = await withTaskGroup(of: (Int, GeneratorReply).self) { group in
                for (i, request) in round.requests.enumerated() {
                    group.addTask { (i, await send(request)) }
                }
                var answered = [GeneratorReply?](repeating: nil, count: round.requests.count)
                for await (i, reply) in group {
                    answered[i] = reply
                }
                return answered.map { $0 ?? GeneratorReply(status: 0, body: Data("no answer".utf8)) }
            }
            for reply in replies {
                var args: [String: JSONValue] = [
                    "status": .number(Double(reply.status)),
                    "statusText": .string(
                        reply.status == 0 ? reply.text : HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                ]
                if let after = reply.after { args["after"] = .string(after) }
                let _: JSONValue = try step("painted", args, bytes: reply.status == 0 ? Data() : reply.body)
            }
            let wait: Wait = try step("round")
            if wait.wait > 0 { try await Task.sleep(nanoseconds: UInt64(wait.wait * 1_000_000_000)) }
        }
        let finished: Finished = try step("finish")
        hear(.painted(pictures: finished.pictures, unmade: finished.unmade))
    }

    /// The deck `brief` asks for, grounded in the sources read, written by `provider`'s `model`
    /// with `key`, on `arc` where one is named, in the theme that ships as `theme`, its pictures
    /// made by `painter` where one is given, and shown to the model as it is drawn unless `look`
    /// says not (PLAN G.13): the bundle made, opened, and what it came to. `hear` hears how far it
    /// has come.
    public func generate(
        brief: Brief, arc: String? = nil, provider: Provider, model: String, base: String? = nil, key: String,
        theme: String, painter: Painter? = nil, look: Bool = true, hear: (GeneratingStep) -> Void
    ) async throws -> (ScaenaSession, Generated) {
        try await prepare(language: brief.language, theme: theme, hear: hear)
        var args = try asking(brief: brief, provider: provider, model: model, base: base)
        if let arc { args["arc"] = .string(arc) }
        let _: JSONValue = try step("begin", args)
        while true {
            let standing: Standing = try step("request", ["key": .string(key)])
            guard let request = standing.request else { break }
            hear(standing.step == "arc" ? .choosing : .writing(slides: standing.slides ?? 0))
            let reply = await send(request)
            let now: Standing = try step(
                "answer",
                [
                    "status": .number(Double(reply.status)),
                    "statusText": .string(reply.status == 0 ? reply.text : HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                    "body": .string(reply.status == 0 ? "" : reply.text),
                ])
            if standing.step == "arc" {
                hear(.chose(arc: now.arc ?? "", fallback: now.fallback))
            } else {
                hear(.wrote(slides: now.written ?? 0, truncated: now.truncated ?? false))
            }
        }
        if let painter {
            try await paint(painter, theme: theme, hear: hear)
        }
        hear(.making)
        let making: Making = try step("make", ["theme": .string(theme), "key": .string(key), "look": .bool(look)])
        var fit = making.fit
        // The deck shown to the model as it is drawn (PLAN G.13), each slide's change kept where
        // lint finds the slide no worse; then the fit, asked after it.
        if let seeing = making.look {
            hear(.looking(slides: seeing.slides))
            let reply = await send(seeing.request)
            let looking: Looking = try step(
                "looked",
                [
                    "status": .number(Double(reply.status)),
                    "statusText": .string(reply.status == 0 ? reply.text : HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                    "body": .string(reply.status == 0 ? "" : reply.text),
                    "key": .string(key),
                ])
            if let why = looking.unlooked { hear(.unlooked(why)) } else { hear(.looked(slides: looking.looked)) }
            fit = looking.fit
        }
        if let fit {
            hear(.fitting(texts: fit.texts))
            let reply = await send(fit.request)
            let fitting: Fitting = try step(
                "fitted",
                [
                    "status": .number(Double(reply.status)),
                    "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                    "body": .string(reply.text),
                ])
            if let n = fitting.fitted { hear(.fitted(texts: n)) }
            if let why = fitting.unfitted { hear(.unfitted(why)) }
        }
        let generated: Generated = try step("report")
        var error: UnsafeMutablePointer<CChar>?
        guard let session = scaena_generator_open(handle, &error) else { throw ScaenaError.taking(error) }
        return (ScaenaSession(handle: session), generated)
    }
}
