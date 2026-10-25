import Foundation
import Observation

/// The assistant (PLAN 3.6, SPEC §11): the user's question, with their own key, to the model they
/// chose; each tool the model calls runs on the deck the window edits, as the browser's assistant
/// runs it, until the model answers, the user stops it, or the question has taken its rounds.
/// Each edit it makes shows as it is made, and the window's undo takes it back.
@MainActor
@Observable
public final class Assistant {
    /// What the conversation shows, in order.
    public struct Entry: Identifiable, Sendable {
        public enum Kind: Sendable {
            /// The user asked.
            case question(String)
            /// The model said.
            case said(String)
            /// The model called a tool; once it ran, what it came to and the frame it drew.
            case call(name: String, args: JSONValue, ran: Ran?)
            /// The question is answered, or stopped: why (`end`, `length`, `stopped`, `steps`).
            case done(String)
            /// The provider refused, or could not be reached.
            case failed(String)
        }

        public let id: Int
        public var kind: Kind
    }

    /// How a request is made, and what it answered: its body and status.
    public typealias Send = @Sendable (URLRequest) async throws -> (Data, Int)

    /// How a request is made whose answer is read as it is written (narrative ATTR-3): each
    /// piece of the body handed to `heard` as it arrives, and the whole body and status at the end.
    public typealias Stream = @Sendable (URLRequest, _ heard: @Sendable (String) async -> Void) async throws -> (Data, Int)

    /// A request made with `URLSession`, under the app's sandbox and the system's proxies.
    nonisolated public static let urlSession: Send = { request in
        let answered = try await Assistant.urlSessionAsking(request)
        return (answered.data, answered.status)
    }

    /// What a turn's request came to: its body, its status, and the seconds its `Retry-After` asks
    /// the client to wait, where it asks (lift-up REL-5).
    public struct Answered: Sendable {
        public let data: Data
        public let status: Int
        public let after: TimeInterval?

        public init(data: Data, status: Int, after: TimeInterval? = nil) {
            self.data = data
            self.status = status
            self.after = after
        }

        /// `response`'s status and `Retry-After`, as seconds or a date, with `data`.
        init(_ data: Data, _ response: URLResponse) {
            let http = response as? HTTPURLResponse
            self.init(
                data: data, status: http?.statusCode ?? 0,
                after: Answered.seconds(http?.value(forHTTPHeaderField: "retry-after")))
        }

        /// The seconds `header` asks the client to wait, as the browser reads `Retry-After`: a
        /// number of seconds, or a date; none where it says neither.
        public static func seconds(_ header: String?, now: Date = Date()) -> TimeInterval? {
            guard let header = header?.trimmingCharacters(in: .whitespaces), !header.isEmpty else { return nil }
            if let seconds = Double(header), seconds.isFinite { return max(0, seconds) }
            let format = DateFormatter()
            format.locale = Locale(identifier: "en_US_POSIX")
            format.timeZone = TimeZone(identifier: "GMT")
            format.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
            return format.date(from: header).map { max(0, $0.timeIntervalSince(now)) }
        }
    }

    /// A turn's request made as `Send` makes it, its `Retry-After` read too.
    public typealias Asking = @Sendable (URLRequest) async throws -> Answered

    /// A turn's request made as `Stream` makes it, its `Retry-After` read too.
    public typealias Streaming = @Sendable (URLRequest, _ heard: @Sendable (String) async -> Void) async throws -> Answered

    /// A turn's request made with `URLSession`, read whole, its `Retry-After` read.
    nonisolated public static let urlSessionAsking: Asking = { request in
        let (data, response) = try await URLSession.shared.data(for: request)
        return Answered(data, response)
    }

    /// A request made with `URLSession`, its answer read line by line as it arrives.
    nonisolated public static let urlSessionStream: Stream = { request, heard in
        let answered = try await Assistant.urlSessionStreaming(request, heard)
        return (answered.data, answered.status)
    }

    /// A turn's request made with `URLSession`, its answer read line by line as it arrives, its
    /// `Retry-After` read.
    nonisolated public static let urlSessionStreaming: Streaming = { request, heard in
        let (bytes, response) = try await URLSession.shared.bytes(for: request)
        var whole = Data()
        var line = Data()
        for try await byte in bytes {
            line.append(byte)
            if byte == 0x0A {
                whole.append(line)
                await heard(String(decoding: line, as: UTF8.self))
                line.removeAll(keepingCapacity: true)
            }
        }
        if !line.isEmpty {
            whole.append(line)
            await heard(String(decoding: line, as: UTF8.self))
        }
        return Answered(whole, response)
    }

    public let editor: DeckEditor
    public private(set) var entries: [Entry] = []
    /// Whether a question is being answered: while one is, the deck is the assistant's alone
    /// (lift-up STAB-5).
    public private(set) var working = false {
        didSet { editor.assisting = working }
    }
    /// Tokens in and out since the conversation began.
    public private(set) var usage = Usage(input: 0, output: 0)
    /// A question another panel drafted for the user to send (the story's, PLAN G.9): the
    /// assistant's panel takes it into its box, and it is gone from here.
    public var drafted: String?
    /// Why the provider is being asked again, and when, while it is (narrative REL-3): "Anthropic
    /// is busy. Trying again in 4 s (2 of 3)…"; none while nothing waits.
    public private(set) var waiting: String?
    /// How long an answer, once it has begun, may go with nothing more from the provider before it
    /// is asked again: two minutes; a test shortens it. The wait for its first line is the
    /// request's own timeout, ten minutes, since a model that reasons first may say nothing for
    /// longer than two.
    public var silence: TimeInterval = 120
    /// The waits before asking again, in seconds; a test shortens them.
    public var waits: [TimeInterval] = [2, 4, 8]
    /// Hears each change the assistant makes to the deck: the source it replaced and the files it
    /// wrote beside the deck, for an undo.
    @ObservationIgnored public var edited: (_ before: String, _ files: [Rewritten]) -> Void = { _, _ in }
    /// The Geoapify key `map_fetch` fetches a map with (ADR-0030, PLAN O.1d): the Keychain's.
    @ObservationIgnored public var mapKey: () -> String? = { Keychain().geoapify }
    /// How `map_fetch` makes each of Geoapify's requests: `URLSession` in the app.
    @ObservationIgnored public var mapSend: Generator.Send = { await GeneratorReply.made($0) }

    private let send: Asking
    private let stream: Streaming?
    @ObservationIgnored private var chat: ScaenaChat?
    /// The question being answered: what a test awaits.
    @ObservationIgnored private(set) var asking: Task<Void, Never>?
    @ObservationIgnored private var counted = 0
    /// The entry the answer being written goes into, until it is whole (narrative ATTR-3).
    @ObservationIgnored private var saying: Int?

    /// The assistant on `editor`'s deck, its requests made by `stream`, which reads each answer as
    /// it is written (narrative ATTR-3), else by `send`, which reads it whole: `URLSession`'s in
    /// the app, a stand-in in a test.
    public convenience init(editor: DeckEditor, send: @escaping Send, stream: Stream?) {
        self.init(
            editor: editor,
            asking: { request in
                let (data, status) = try await send(request)
                return Answered(data: data, status: status)
            },
            streaming: stream.map { stream -> Streaming in
                { request, heard in
                    let (data, status) = try await stream(request, heard)
                    return Answered(data: data, status: status)
                }
            })
    }

    /// The assistant on `editor`'s deck, its requests made by `streaming` where it reads each
    /// answer as it is written, else by `asking`, each with the `Retry-After` its answer said
    /// (lift-up REL-5).
    public init(editor: DeckEditor, asking: @escaping Asking, streaming: Streaming?) {
        self.editor = editor
        self.send = asking
        self.stream = streaming
    }

    /// The assistant on `editor`'s deck, as the app makes it: each answer read as it is written.
    public convenience init(editor: DeckEditor) {
        self.init(editor: editor, asking: Assistant.urlSessionAsking, streaming: Assistant.urlSessionStreaming)
    }

    /// The assistant on `editor`'s deck, each answer read whole by `send`: a test's stand-in.
    public convenience init(editor: DeckEditor, send: @escaping Send) {
        self.init(editor: editor, send: send, stream: nil)
    }

    /// Ask `text` of `model`, of `provider`'s, with `key`: begun with what the window shows. A
    /// question asked while one is answered waits for nothing: it is not asked.
    public func ask(
        _ text: String, provider: Provider, model: String, key: String, base: String? = nil, seeing: Seeing? = nil
    ) {
        guard !working, !text.isEmpty else { return }
        do {
            let chat = try conversing(provider: provider, model: model, base: base)
            try chat.ask(text, seeing: seeing, on: editor.session)
        } catch {
            add(.failed(Why.said(error, of: provider)))
            return
        }
        add(.question(text))
        working = true
        asking = Task { await converse(key: key) }
    }

    /// Stop the question being answered: a call not yet run is answered as stopped.
    public func stop() {
        asking?.cancel()
    }

    /// Start again: the next question is the first.
    public func forget() {
        stop()
        chat?.forget()
        entries = []
        usage = Usage(input: 0, output: 0)
    }

    /// The conversation, its model `model` from now on: a model switched keeps it.
    private func conversing(provider: Provider, model: String, base: String?) throws -> ScaenaChat {
        if let chat {
            if chat.provider != provider || chat.model != model || chat.base != base {
                try chat.use(provider: provider, model: model, base: base)
            }
            return chat
        }
        let made = try ScaenaChat(provider: provider, model: model, base: base)
        chat = made
        return made
    }

    /// The model asked, and each call it makes run, until it answers.
    private func converse(key: String) async {
        defer {
            working = false
            asking = nil
        }
        guard let chat else { return }
        do {
            while true {
                try Task.checkCancellation()
                let request = try chat.request(key: key).urlRequest()
                let next: Next
                do {
                    next = try await turn(request, chat: chat)
                } catch let refusal as Refusal {
                    // Too long for the model: asked again once with half as much of tools'
                    // results (lift-up PERF-3), before the person is told to start again.
                    if refusal.tooLong, try chat.shorter() { continue }
                    // A refusal, or an answer that does not read, said in a person's words (PLAN
                    // 3.18), never the provider's own: the conversation is as it was.
                    unsaid()
                    add(.failed(refusal.said))
                    return
                }
                switch next {
                case .done(let text, let stop, let used):
                    count(used)
                    said(text)
                    add(.done(stop))
                    return
                case .calls(let text, let calls, let used):
                    count(used)
                    said(text)
                    for (index, call) in calls.enumerated() {
                        // A call the user stopped before it ran is answered as stopped.
                        if Task.isCancelled { break }
                        let at = add(.call(name: call.name, args: call.args, ran: nil))
                        // A map is fetched here, as Insert › Map… fetches one: the client makes
                        // Geoapify's requests (PLAN O.1d).
                        let ran: Ran
                        if call.name == "map_fetch" {
                            ran = try await mapFetch(index, args: call.args, chat: chat)
                        } else if ScaenaChat.readsAlone(call) {
                            ran = try await apart(index, call: call, chat: chat)
                        } else {
                            ran = try chat.run(index, on: editor.session)
                        }
                        entries[at].kind = .call(name: call.name, args: call.args, ran: ran)
                        if ran.edited || !ran.rewritten.isEmpty, let before = editor.reread() {
                            edited(before, ran.rewritten)
                        }
                    }
                    guard try chat.next() else {
                        add(.done("steps"))
                        return
                    }
                }
            }
        } catch is CancellationError {
            settle(chat)
        } catch let error as URLError where error.code == .cancelled {
            settle(chat)
        } catch {
            if Task.isCancelled { settle(chat) } else { add(.failed(Why.said(error, of: chat.provider))) }
        }
    }

    /// The last answer's call `index`, which only reads the deck (`deck_lint`, `deck_render`,
    /// `deck_health`), run away from the window on a session of a snapshot of the deck as it is,
    /// where the editor reads the deck's health (`OffMain`, narrative lift-up PERF-1, PERF-2):
    /// laying every state out or drawing a frame takes the window's actor for none of it, so the
    /// window answers, and the sweep goes on, while it works.
    private func apart(_ index: Int, call: ToolCall, chat: ScaenaChat) async throws -> Ran {
        let snapshot = try editor.session.snapshot()
        let model = chat.model
        let ran = try await editor.offMain.run {
            try ScaenaChat.runApart(call, model: model, on: try snapshot.opened())
        }
        return try chat.took(index, ran: ran)
    }

    /// `map_fetch` (ADR-0030, PLAN O.1d), the last answer's call `index`, run as Insert › Map… runs:
    /// where the map goes, by the session (`into`, else the state's empty place for a picture, else
    /// `at`); each request made here with the Geoapify key kept; and the map put there as one
    /// `deck_patch` by the model's agent, its picture in the bundle. A name that matches several
    /// places answers them, to ask again with `pick`; a refresh asks for what its map asked. Its
    /// answer is the MCP tool's, as the browser's assistant gives it.
    private func mapFetch(_ index: Int, args: JSONValue, chat: ScaenaChat) async throws -> Ran {
        func answer(_ value: JSONValue, error: Bool = false, edited: Bool = false) throws -> Ran {
            let json = String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
            return try chat.ran(index, json: json, error: error, edited: edited)
        }
        func stop(_ message: String) throws -> Ran { try answer(["message": .string(message)], error: true) }
        guard let key = mapKey() else {
            return try stop("no Geoapify key is kept: the person keeps one in Settings, from geoapify.com")
        }
        let session = editor.session
        do {
            let target: MapTarget
            var ask: MapAsk
            var label = args["label"]?.string
            if let refresh = args["refresh"]?.string {
                if args["place"]?.string != nil {
                    return try stop("a refresh asks for what its map asked: name no place")
                }
                guard let state = try args["state"]?.string ?? session.states().first else {
                    return try stop("the deck has no state")
                }
                let center = args["center"]?.array
                let again = try session.mapRefreshing(
                    state: state, node: refresh, zoom: args["zoom"]?.number, lat: center?.first?.number,
                    lon: center?.dropFirst().first?.number)
                target = again.target
                ask = MapAsk(again: again, zoom: again.zoom, center: again.center)
                label = label ?? again.label ?? ""
            } else {
                guard let place = args["place"]?.string, !place.isEmpty else {
                    return try stop("say where: an address, a place, or `lat, lon`")
                }
                var aim: [String: JSONValue] = [:]
                for name in ["state", "into", "at"] { aim[name] = args[name] }
                target = try session.call("mapAiming", .object(["aim": .object(aim)]))
                if args["pick"]?.number == 0 { return try stop("pick a place from 1, as they are listed") }
                let show = args["show"]?.string.flatMap(MapShow.init(rawValue:)) ?? .neighborhood
                ask = MapAsk(place: place, show: show, target: target, pick: args["pick"]?.number.map { Int($0) - 1 })
                ask.zoom = args["zoom"]?.number
                ask.list = args["places"]?.bool ?? false
            }
            if let style = args["style"]?.string { ask.style = style }
            switch try await Generator(send: mapSend).map(ask, key: key) {
            case .places(let places):
                return try answer(["target": try JSONValue.encoded(target), "places": try JSONValue.encoded(places)])
            case .map(let map):
                let put = try session.mapping(map, at: target, label: label)
                let called = try session.tool("deck_patch", ["ops": .array(put.patch)], author: "agent:\(chat.model)")
                var placed: [String: JSONValue] = [
                    "node": .string(put.node), "src": .string(put.src), "place": .string(put.place),
                    "said": .string(put.said),
                ]
                for (name, value) in called.result.object ?? [:] where placed[name] == nil { placed[name] = value }
                return try answer(
                    ["target": try JSONValue.encoded(target), "placed": .object(placed)], edited: called.edited)
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as ScaenaError {
            return try stop(error.message)
        }
    }

    /// A provider's answer that is no answer, in a person's words.
    private struct Refusal: Error {
        let said: String
        /// Whether what was asked is more than the model takes at once.
        var tooLong = false
    }

    /// When the answer being read last said something: never, until its first line.
    private actor Lines {
        private var last: ContinuousClock.Instant?

        func heard() {
            last = .now
        }

        /// Whether the answer has begun and said nothing for `silence` since.
        func quiet(for silence: TimeInterval) -> Bool {
            guard let last else { return false }
            return ContinuousClock.now - last > .seconds(silence)
        }
    }

    /// One turn of the model: `request` made and its answer read, and, where the provider is busy
    /// (408, 429), down (5xx), or silent past its deadline, asked again after a wait, up to
    /// `waits.count` times, the window told of each wait (narrative REL-3). A refusal that is
    /// not worth asking again, a key not taken or a question too long, is thrown as a `Refusal`;
    /// the user's stop, as a cancellation.
    private func turn(_ request: URLRequest, chat: ScaenaChat) async throws -> Next {
        defer { waiting = nil }
        for attempt in 0... {
            saying = nil
            let again: String
            // The seconds the provider asked to be given, where it asked (lift-up REL-5).
            var asked: TimeInterval?
            do {
                let reply = try await answered(request, chat: chat)
                let (data, status) = (reply.data, reply.status)
                asked = reply.after
                try Task.checkCancellation()
                let body = String(decoding: data, as: UTF8.self)
                do {
                    return try chat.answer(status: status, body: body)
                } catch {
                    let why = Why.Answer(status: status, body: body)
                    guard why == .busy || why == .down else {
                        throw Refusal(
                            said: Why.answered(chat.provider, status: status, body: body), tooLong: why == .tooLong)
                    }
                    again = why == .busy ? "\(chat.provider.name) is busy." : "\(chat.provider.name) couldn't answer just now."
                }
            } catch let error as URLError where error.code == .timedOut {
                again = "\(chat.provider.name) went quiet."
            }
            try Task.checkCancellation()
            guard attempt < waits.count else {
                throw Refusal(said: again + " Try again in a moment.")
            }
            unsaid()
            // The longer of what the provider asked and this attempt's own wait, a minute at most,
            // as the browser waits (lift-up REL-5).
            let wait = min(60, max(asked ?? 0, waits[attempt]))
            waiting = "\(again) Trying again in \(Int(wait)) s (\(attempt + 1) of \(waits.count))…"
            try await Task.sleep(for: .seconds(wait))
        }
        fatalError("a turn ends by returning or throwing")
    }

    /// `request` made and its answer read: as it is written where the assistant streams, each
    /// piece's words shown as they come (narrative ATTR-3); else whole.
    /// Once the answer has begun, `silence` with no new line gives it up as timed out (narrative
    /// REL-3); the wait for its first line is the request's own timeout.
    private func answered(_ request: URLRequest, chat: ScaenaChat) async throws -> Answered {
        guard let stream else { return try await send(request) }
        let lines = Lines()
        let silence = silence
        return try await withThrowingTaskGroup(of: Answered?.self) { group in
            group.addTask {
                // The closure crosses to the session's task, so it carries nothing of the
                // conversation: each piece is read by the conversation the assistant holds, on the
                // main actor.
                try await stream(request) { chunk in
                    await lines.heard()
                    await self.heard(chunk)
                }
            }
            group.addTask {
                repeat {
                    try await Task.sleep(for: .seconds(silence / 4))
                    if await lines.quiet(for: silence) { throw URLError(.timedOut) }
                } while !Task.isCancelled
                return nil
            }
            defer { group.cancelAll() }
            guard let answer = try await group.next() ?? nil else { throw URLError(.timedOut) }
            return answer
        }
    }

    /// A piece of the answer, as it arrives: the words it completes shown in the entry the answer
    /// is written into.
    private func heard(_ chunk: String) {
        guard let chat, let words = try? chat.heard(chunk), !words.isEmpty else { return }
        if let at = saying, case .said(let sofar) = entries[at].kind {
            entries[at].kind = .said(sofar + words)
        } else {
            saying = add(.said(words))
        }
    }

    /// The answer's words, whole: in the entry they were written into, or a new one.
    private func said(_ text: String) {
        defer { saying = nil }
        if let at = saying {
            if text.isEmpty { entries.remove(at: at) } else { entries[at].kind = .said(text) }
        } else if !text.isEmpty {
            add(.said(text))
        }
    }

    /// The words written so far taken away: the answer did not read.
    private func unsaid() {
        if let at = saying { entries.remove(at: at) }
        saying = nil
    }

    /// A question stopped: every call of the last answer answered, as stopped where it never ran.
    private func settle(_ chat: ScaenaChat) {
        saying = nil
        _ = try? chat.next()
        add(.done("stopped"))
    }

    private func count(_ used: Usage?) {
        guard let used else { return }
        usage = Usage(input: usage.input + used.input, output: usage.output + used.output)
    }

    /// Add `kind` to the conversation shown: where it stands.
    @discardableResult
    private func add(_ kind: Entry.Kind) -> Int {
        counted += 1
        entries.append(Entry(id: counted, kind: kind))
        return entries.count - 1
    }
}

/// What a call to each of the assistant's tools does, in a person's words (PLAN 3.18): what the
/// assistant is doing while the call runs, and what it did once it has, or that it did not. No
/// tool's name reaches the window.
public enum ToolWords {
    /// Each tool's words, as the MCP server names the tools (`docs/schema/mcp`): while it runs,
    /// once it has, and where it did not (stopped, or an edit refused or only tried).
    private static let words: [String: (doing: String, done: String, not: String)] = [
        "background_set": ("Setting a background…", "Set a background", "Didn't set a background"),
        "data_attach": ("Adding data…", "Added data", "Didn't add data"),
        "data_edit": ("Changing the data…", "Changed the data", "Didn't change the data"),
        "deck_create": ("Making a deck…", "Made a deck", "Didn't make a deck"),
        "deck_diff": ("Comparing versions…", "Compared versions", "Didn't compare versions"),
        "deck_export": ("Exporting the deck…", "Exported the deck", "Didn't export the deck"),
        "deck_find": ("Searching the deck…", "Searched the deck", "Didn't search the deck"),
        "deck_generate": ("Making a deck from slides…", "Made a deck from slides", "Didn't make a deck from slides"),
        "deck_health": ("Reading the deck's story…", "Read the deck's story", "Didn't read the deck's story"),
        "deck_history": ("Reading the history…", "Looked at the history", "Didn't look at the history"),
        "deck_inspect": ("Looking over the deck…", "Looked over the deck", "Didn't look over the deck"),
        "deck_lint": ("Checking the deck…", "Checked the deck", "Didn't check the deck"),
        "deck_patch": ("Editing the deck…", "Edited the deck", "Didn't edit the deck"),
        "deck_read": ("Reading the deck…", "Went through the deck", "Didn't go through the deck"),
        "deck_render": ("Looking at a slide…", "Looked at a slide", "Didn't look at a slide"),
        "icon_find": ("Finding an icon…", "Found an icon", "Didn't find an icon"),
        "image_add": (
            "Putting a picture on the slide…", "Put a picture on the slide", "Didn't put a picture on the slide"
        ),
        "map_fetch": ("Fetching a map…", "Put a map on the slide", "Didn't fetch a map"),
        "slide_write": ("Writing a slide…", "Wrote a slide", "Didn't write a slide"),
        "spine_read": ("Reading the outline…", "Went through the outline", "Didn't go through the outline"),
        "spine_update": ("Changing the outline…", "Changed the outline", "Didn't change the outline"),
        "state_start": ("Adding a slide…", "Added a slide", "Didn't add a slide"),
        "theme_apply": ("Changing the theme…", "Changed the theme", "Didn't change the theme"),
        "theme_edit": ("Editing the theme…", "Edited the theme", "Didn't edit the theme"),
        "resource_read": (
            "Reading how decks are written…", "Read how decks are written", "Didn't read how decks are written"
        ),
    ]

    /// What the assistant is doing while a call to `tool` runs.
    public static func doing(_ tool: String) -> String { words[tool]?.doing ?? "Working…" }

    /// What a call to `tool` did, once it ran.
    public static func done(_ tool: String) -> String { words[tool]?.done ?? "Used a tool" }

    /// What a call to `tool` did not do, once it ran and stopped, or its edit was refused or only
    /// tried.
    public static func undone(_ tool: String) -> String { words[tool]?.not ?? "Didn't use a tool" }

    /// How a call `ran` is headed: what it did, or, where it did not do it, that it did not.
    public static func ran(_ ran: Ran) -> String { ran.made ? done(ran.name) : undone(ran.name) }

    /// Whether `tool` has words of its own, not the ones any tool falls back on.
    public static func knows(_ tool: String) -> Bool { words[tool] != nil }
}
