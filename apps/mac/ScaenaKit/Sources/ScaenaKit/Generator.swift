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
    /// The themes that ship, to make the deck in, each with its family (PLAN T.0d).
    public let themes: [ShippedTheme]
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

/// The deck's outline before its slides (narrative FUNC-1): the big idea, and each beat with the one
/// claim its slides must leave the audience with and the slides it takes, as the engine's
/// `scaena_narrato::Outline` keeps it.
public struct StoryOutline: Codable, Sendable, Equatable {
    public struct Beat: Codable, Sendable, Equatable {
        /// Its name: the arc's, or the person's own for a beat they added.
        public var name: String
        /// The one sentence its slides must leave the audience with.
        public var claim: String
        /// The slides it takes, as the plan spreads them.
        public var slides: Int

        public init(name: String, claim: String, slides: Int = 0) {
            self.name = name
            self.claim = claim
            self.slides = slides
        }
    }

    public var bigIdea: String
    public var beats: [Beat]

    public init(bigIdea: String, beats: [Beat]) {
        self.bigIdea = bigIdea
        self.beats = beats
    }
}

/// The outline as the model wrote it, shown for the person to settle (narrative FUNC-1): on `arc`,
/// chosen for `reasoning`, or the heuristic's where `fallback` says why.
public struct Outlined: Sendable, Equatable {
    public let outline: StoryOutline
    public let arc: String
    public let reasoning: String
    public let fallback: String?

    public init(outline: StoryOutline, arc: String, reasoning: String, fallback: String? = nil) {
        self.outline = outline
        self.arc = arc
        self.reasoning = reasoning
        self.fallback = fallback
    }

    /// An outline as one reads, for a window shown with none written: what the screenshots CI keeps
    /// show of the step.
    public static let sample = Outlined(
        outline: StoryOutline(
            bigIdea: "A trail is kept by the people who walk it.",
            beats: [
                .init(name: "Problem: Define core challenge and stakes", claim: "The storm took fifty-two miles of trail in one night.", slides: 1),
                .init(name: "Impact: Quantify cost of inaction", claim: "Every closed mile costs the valley a season of visitors.", slides: 1),
                .init(name: "Existing Approaches: Why they fall short", claim: "Contract crews cannot reach the high sections before the snow.", slides: 1),
                .init(name: "Solution: Introduce your answer", claim: "Volunteers rebuilt it in one summer, a mile a weekend.", slides: 1),
                .init(name: "Proof: Case studies, data, demos", claim: "Ninety thousand people walked it again this year.", slides: 2),
                .init(name: "Call to Action: Next steps", claim: "Your gift keeps the crews on the trail next summer.", slides: 2),
            ]),
        arc: "Problem–Solution",
        reasoning: "Donors know the trail; the talk moves them from the loss to what their gift keeps open.")
}

/// What the person decided of the outline shown: settled, so the slides are written to it; or
/// another arc, by name or the heuristic's where none, to outline again.
public enum OutlineDecision: Sendable, Equatable {
    case settled(StoryOutline)
    case anotherArc(String?)
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
    /// The outline before the slides (narrative FUNC-1): asked, written with this many beats, and
    /// settled by the person with this many.
    case outlining
    case outlined(beats: Int)
    case settled(beats: Int)
    case writing(slides: Int)
    case retrying(seconds: Double, tried: Int)
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
        case .outlining: "Outlining the story: the big idea, and what each beat says…"
        case .outlined(let beats): "Outline: \(beats) beats. Read it, change what you will, then Write the Slides."
        case .settled(let beats): "Outline settled: \(beats) beats"
        case .writing(let slides): "Writing the slides, at least \(slides)…"
        case .retrying(let seconds, let tried):
            "The provider is busy or did not answer: asking again in \(Int(seconds.rounded())) s (try \(tried + 1) of 6)…"
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

    /// `request` made by `URLSession`: status 0 where it never came back, saying why. A redirect is
    /// followed only where ``follows(_:)`` lets it, else answered as it came.
    public static func made(_ request: GeneratorRequest, by session: URLSession = .shared) async -> GeneratorReply {
        do {
            let (data, response): (Data, URLResponse)
            if let most = request.most {
                // A source's body read no further than its plan's `most`, as it arrives: a page or a
                // redirect a person names can stream gigabytes, and the app would run out of memory
                // before a check after the whole (the 20261024 plan's SEC-2), as the CLI reads it.
                let (bytes, answered) = try await session.bytes(for: try request.urlRequest(), delegate: Redirects())
                let tooMuch = GeneratorReply(
                    status: 0, kind: "", body: Data("\(request.url) answered with more than \(most >> 20) MB".utf8))
                if answered.expectedContentLength > Int64(most) {
                    bytes.task.cancel()
                    return tooMuch
                }
                var read = Data()
                for try await byte in bytes {
                    read.append(byte)
                    if read.count > most {
                        bytes.task.cancel()
                        return tooMuch
                    }
                }
                (data, response) = (read, answered)
            } else {
                (data, response) = try await session.data(for: try request.urlRequest(), delegate: Redirects())
            }
            let http = response as? HTTPURLResponse
            return GeneratorReply(
                status: http?.statusCode ?? 0, kind: http?.value(forHTTPHeaderField: "content-type") ?? "", body: data,
                after: http?.value(forHTTPHeaderField: "retry-after"))
        } catch {
            return GeneratorReply(status: 0, kind: "", body: Data(error.localizedDescription.utf8))
        }
    }

    var text: String { String(decoding: body, as: UTF8.self) }

    /// Whether a redirect to `url` is followed: to an `https` address by a name of more than one
    /// label, never an address written as numbers or `localhost`, so a public page a source names
    /// cannot send the app on to this machine or its network (as `scaena_sources::fetch::public`
    /// holds the address a source names itself; lift-up SEC-2).
    public static func follows(_ url: URL) -> Bool {
        guard url.scheme?.lowercased() == "https", var host = url.host(percentEncoded: false)?.lowercased() else {
            return false
        }
        if host.hasSuffix(".") { host.removeLast() }
        let last = host.split(separator: ".").last.map(String.init) ?? ""
        let numbers = last.allSatisfy(\.isNumber) || last.hasPrefix("0x") || host.contains(":")
        return host.contains(".") && !numbers && host != "localhost" && !host.hasSuffix(".localhost")
    }
}

/// Holds each redirect a request meets to ``GeneratorReply/follows(_:)``.
private final class Redirects: NSObject, URLSessionTaskDelegate, Sendable {
    func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest
    ) async -> URLRequest? {
        guard let url = request.url, GeneratorReply.follows(url) else { return nil }
        return request
    }
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
    let send: Send
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

    /// The engine's generation, freed with it. Sendable: its steps are made one at a time, each
    /// awaited before the next, on the main actor or, for the slow ones, on a thread of their own.
    private final class Made: @unchecked Sendable {
        let pointer = scaena_generator_new()!

        deinit {
            scaena_generator_free(pointer)
        }
    }

    /// What a brief may ask.
    nonisolated public static func choices() throws -> BriefChoices {
        try ScaenaSession.decode(scaena_generator_choices(), as: BriefChoices.self)
    }

    /// The stack a slow step is made on: 16 MB, twice the Mac main thread's. Building a deck and drawing
    /// its slides recurses deeper than a dispatch queue's 512 KB worker stack allows in a debug
    /// build (the generator's FFI test overflows at 512 KB and passes from 1 MB).
    nonisolated static let stack = 16 << 20

    /// For a test: told each step `stepAway` makes, on the thread it makes it on.
    var away: (@Sendable (String) -> Void)?

    /// `step`, made on a thread of its own, away from the main actor: the steps that build the deck
    /// and draw every slide (`make`, `looked`, `fitted`), which take seconds for a deck of 20 to 45
    /// slides, while the brief's window goes on answering and its steps go on showing (lift-up
    /// narrative PERF-3). One step at a time: each is awaited before the next is made.
    func stepAway<T: Decodable & Sendable>(_ method: String, _ args: [String: JSONValue] = [:]) async throws -> T {
        let json = String(decoding: try JSONEncoder().encode(JSONValue.object(args)), as: UTF8.self)
        return try await Self.stepped(on: made, method, json, heard: away)
    }

    /// `method` made on `made` on a thread of its own with `stack` bytes of stack, its answer awaited.
    nonisolated private static func stepped<T: Decodable & Sendable>(
        on made: Made, _ method: String, _ json: String, heard: (@Sendable (String) -> Void)?
    ) async throws -> T {
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<T, any Error>) in
            let thread = Thread {
                heard?(method)
                done.resume(with: Result {
                    try ScaenaSession.decode(scaena_generator_call(made.pointer, method, json, nil, 0), as: T.self)
                })
            }
            thread.name = "ScaenaKit.generate"
            thread.stackSize = stack
            thread.qualityOfService = .userInitiated
            thread.start()
        }
    }

    func step<T: Decodable>(_ method: String, _ args: [String: JSONValue] = [:], bytes: Data? = nil) throws -> T {
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
        let reasoning: String?
        let fallback: String?
        let written: Int?
        let truncated: Bool?
        /// The outline, once the model wrote it or the person settled one (narrative FUNC-1).
        let outline: StoryOutline?
        /// The seconds to wait before the same step is asked again, where its last answer was
        /// for now (narrative REL-1).
        let wait: Double?
        let tried: Int?
    }

    private struct Making: Decodable, Sendable {
        struct Look: Decodable, Sendable {
            let request: GeneratorRequest
            let slides: Int
        }

        let look: Look?
        let fit: Fit?
    }

    private struct Fit: Decodable, Sendable {
        let request: GeneratorRequest
        let texts: Int
    }

    /// One round of the look answered: the next round where one is left (narrative PERF-5), else
    /// what the look came to and the fit asked for.
    private struct Looking: Decodable, Sendable {
        let look: Making.Look?
        let looked: Int?
        let unlooked: String?
        let fit: Fit?
    }

    /// A rewrite answered: what it came to, and the next round's request where the rewrite left
    /// a text too long (STAB-4).
    private struct Fitting: Decodable, Sendable {
        let fitted: Int?
        let unfitted: String?
        let fit: Fit?
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

    /// What `painter`'s key lists: its best image model, as the engine orders them, or none,
    /// saying why.
    private enum Listed {
        case model(String)
        case none(why: String)
    }

    /// The best image model `painter`'s key lists, as the engine orders them; none where it
    /// lists none, or where the listing fails, said in a person's words (PLAN 3.18): the deck is
    /// made without pictures, as the browser's and the CLI's are (narrative REL-4), never lost
    /// to its pictures' key.
    private func bestModel(_ painter: Painter) async throws -> Listed {
        var listing: [String: JSONValue] = ["provider": .string(painter.provider.rawValue), "key": .string(painter.key)]
        if let base = painter.base { listing["base"] = .string(base) }
        let list: Listing = try step("imageModels", listing)
        let listed = await send(list.request)
        // A key the provider does not take, no way to it, or an answer that does not read, said in
        // a person's words (PLAN 3.18), not in the provider's own nor in what the answer holds.
        let unlisted = Listed.none(why: Why.answered(painter.provider, status: listed.status, body: listed.text))
        guard (200..<300).contains(listed.status) else { return unlisted }
        let models: Models
        do {
            models = try step(
                "imageModelsRead",
                [
                    "provider": .string(painter.provider.rawValue), "status": .number(Double(listed.status)),
                    "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: listed.status)),
                    "body": .string(listed.text),
                ])
        } catch {
            return unlisted
        }
        guard let model = models.models.first else {
            return .none(why: "your \(painter.provider.name) key lists no image model")
        }
        return .model(model)
    }

    /// The pictures asked for made by `painter`, every one at once, a busy provider asked again
    /// after the wait the engine says: the last answer the provider gave, which says why where a
    /// picture was not made.
    @discardableResult
    private func rounds(_ painter: Painter) async throws -> GeneratorReply? {
        var last: GeneratorReply?
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
                last = reply
            }
            let wait: Wait = try step("round")
            if wait.wait > 0 { try await Task.sleep(nanoseconds: UInt64(wait.wait * 1_000_000_000)) }
        }
        return last
    }

    /// The pictures the narrative's slides ask for, made by `painter`'s best image model, every
    /// one at once, a busy provider asked again after the wait the engine says; kept for `make`.
    private func paint(_ painter: Painter, theme: String, hear: (GeneratingStep) -> Void) async throws {
        let model: String
        switch try await bestModel(painter) {
        case .model(let best): model = best
        case .none(let why):
            hear(.painted(pictures: 0, unmade: [why]))
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
        try await rounds(painter)
        let finished: Finished = try step("finish")
        hear(.painted(pictures: finished.pictures, unmade: finished.unmade))
    }

    private struct Picture: Decodable {
        let bytes: Data
        let ext: String
        let alt: String
    }

    /// One picture a person asks for in an editor (PLAN 2.107): of `subject`, in their words, made
    /// as `painter.look` in the palette of `theme` (the deck's theme, its JSON), for a place `size`
    /// on the slide, which frames it wide, tall, or square, by the best image model `painter`'s key
    /// lists, as a made deck's pictures are. Why none was made, where none was, in a person's words
    /// (PLAN 3.18): the key not accepted, the provider's safety system refusing, the provider busy.
    public func picture(subject: String, theme: String, size: CGSize, painter: Painter) async throws -> MadePicture {
        // One picture is the whole ask here, so a key that lists no image model is the error.
        let model: String
        switch try await bestModel(painter) {
        case .model(let best): model = best
        case .none(let why): throw ScaenaError(message: why)
        }
        var asking: [String: JSONValue] = [
            "theme": .string(theme), "subject": .string(subject), "look": .string(painter.look),
            "width": .number(Double(size.width)), "height": .number(Double(size.height)),
            "provider": .string(painter.provider.rawValue), "model": .string(model),
        ]
        if let base = painter.base { asking["base"] = .string(base) }
        let _: Asked = try step("picture", asking)
        let last = try await rounds(painter)
        let made: Picture
        do {
            made = try step("pictureMade")
        } catch {
            // Why none was made: what the provider answered in its place, said by what it means,
            // never in the provider's words; the engine's reason where no answer came.
            guard let last else { throw error }
            throw ScaenaError(message: Why.answered(painter.provider, status: last.status, body: last.text))
        }
        return MadePicture(bytes: made.bytes, ext: made.ext, alt: made.alt, model: model)
    }

    /// How the person settles the outline shown (narrative FUNC-1): their decision, or none where
    /// they stopped, which stops the deck.
    public typealias Settle = @MainActor (Outlined) async -> OutlineDecision?

    /// The deck `brief` asks for, grounded in the sources read, written by `provider`'s `model`
    /// with `key`, on `arc` where one is named, in the theme that ships as `theme`, its pictures
    /// made by `painter` where one is given, and shown to the model as it is drawn unless `look`
    /// says not (PLAN G.13): the bundle made, opened, and what it came to. `hear` hears how far it
    /// has come. With `outline`, the outline is written first and shown by it, and the slides
    /// follow what the person decided of it (narrative FUNC-1); without, the slides come at once.
    public func generate(
        brief: Brief, arc: String? = nil, provider: Provider, model: String, base: String? = nil, key: String,
        theme: String, painter: Painter? = nil, look: Bool = true, outline: Settle? = nil,
        hear: (GeneratingStep) -> Void
    ) async throws -> (ScaenaSession, Generated) {
        try await prepare(language: brief.language, theme: theme, hear: hear)
        var args = try asking(brief: brief, provider: provider, model: model, base: base)
        if let arc { args["arc"] = .string(arc) }
        let _: JSONValue = try step("begin", args)
        if arc != nil, outline != nil { let _: Standing = try step("outline") }
        while true {
            let standing: Standing = try step("request", ["key": .string(key)])
            guard let request = standing.request else { break }
            hear(standing.step == "arc" ? .choosing : standing.step == "outline" ? .outlining : .writing(slides: standing.slides ?? 0))
            let reply = await send(request)
            var answered: [String: JSONValue] = [
                "status": .number(Double(reply.status)),
                "statusText": .string(reply.status == 0 ? reply.text : HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                "body": .string(reply.status == 0 ? "" : reply.text),
            ]
            if let after = reply.after { answered["after"] = .string(after) }
            let now: Standing = try step("answer", answered)
            // An answer for now (busy, a server's failure, none): the same step asked again after
            // the wait the engine says, up to six times (narrative REL-1); a cancellation ends the
            // wait.
            if let wait = now.wait {
                hear(.retrying(seconds: wait, tried: now.tried ?? 0))
                try await Task.sleep(for: .seconds(wait))
                continue
            }
            switch standing.step {
            case "arc":
                hear(.chose(arc: now.arc ?? "", fallback: now.fallback))
                // The outline before the slides: the next request is the outline's.
                if outline != nil { let _: Standing = try step("outline") }
            case "outline":
                guard let outline, let written = now.outline else { break }
                hear(.outlined(beats: written.beats.count))
                let shown = Outlined(outline: written, arc: now.arc ?? "", reasoning: now.reasoning ?? "", fallback: now.fallback)
                guard let decided = await outline(shown) else { throw CancellationError() }
                switch decided {
                case .settled(let settled):
                    let encoded = try JSONDecoder().decode(JSONValue.self, from: try JSONEncoder().encode(settled))
                    let after: Standing = try step("settle", ["outline": encoded])
                    hear(.settled(beats: after.outline?.beats.count ?? 0))
                case .anotherArc(let name):
                    // Chosen with no call, and outlined again.
                    var choosing: [String: JSONValue] = [:]
                    if let name { choosing["arc"] = .string(name) }
                    let chosen: Standing = try step("chooseArc", choosing)
                    hear(.chose(arc: chosen.arc ?? "", fallback: nil))
                    let _: Standing = try step("outline")
                }
            default:
                hear(.wrote(slides: now.written ?? 0, truncated: now.truncated ?? false))
            }
        }
        if let painter {
            try await paint(painter, theme: theme, hear: hear)
        }
        hear(.making)
        let making: Making = try await stepAway(
            "make", ["theme": .string(theme), "key": .string(key), "look": .bool(look)])
        var fit = making.fit
        // The deck shown to the model as it is drawn (PLAN G.13), each slide's change kept where
        // lint finds the slide no worse; then the fit, asked after it.
        // The look goes a dozen slides a round (narrative PERF-5): each answer asks the next
        // round, until the engine says what the look came to.
        var seeing = making.look
        if let first = seeing { hear(.looking(slides: first.slides)) }
        while let round = seeing {
            let reply = await send(round.request)
            let looking: Looking = try await stepAway(
                "looked",
                [
                    "status": .number(Double(reply.status)),
                    "statusText": .string(reply.status == 0 ? reply.text : HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                    "body": .string(reply.status == 0 ? "" : reply.text),
                    "key": .string(key),
                ])
            seeing = looking.look
            if seeing != nil { continue }
            if let why = looking.unlooked { hear(.unlooked(why)) } else { hear(.looked(slides: looking.looked ?? 0)) }
            fit = looking.fit
        }
        // Each answer may ask once more for what the rewrite left too long (STAB-4).
        while let asked = fit {
            hear(.fitting(texts: asked.texts))
            let reply = await send(asked.request)
            let fitting: Fitting = try await stepAway(
                "fitted",
                [
                    "status": .number(Double(reply.status)),
                    "statusText": .string(HTTPURLResponse.localizedString(forStatusCode: reply.status)),
                    "body": .string(reply.text),
                    "key": .string(key),
                ])
            if let n = fitting.fitted { hear(.fitted(texts: n)) }
            if let why = fitting.unfitted { hear(.unfitted(why)) }
            fit = fitting.fit
        }
        let generated: Generated = try step("report")
        var error: UnsafeMutablePointer<CChar>?
        guard let session = scaena_generator_open(handle, &error) else { throw ScaenaError.taking(error) }
        return (ScaenaSession(handle: session), generated)
    }
}
