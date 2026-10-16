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

    /// A request made with `URLSession`, under the app's sandbox and the system's proxies.
    nonisolated public static let urlSession: Send = { request in
        let (data, response) = try await URLSession.shared.data(for: request)
        return (data, (response as? HTTPURLResponse)?.statusCode ?? 0)
    }

    public let editor: DeckEditor
    public private(set) var entries: [Entry] = []
    /// Whether a question is being answered.
    public private(set) var working = false
    /// Tokens in and out since the conversation began.
    public private(set) var usage = Usage(input: 0, output: 0)
    /// A question another panel drafted for the user to send (the story's, PLAN G.9): the
    /// assistant's panel takes it into its box, and it is gone from here.
    public var drafted: String?
    /// Hears each change the assistant makes to the deck: the source it replaced and the files it
    /// wrote beside the deck, for an undo.
    @ObservationIgnored public var edited: (_ before: String, _ files: [Rewritten]) -> Void = { _, _ in }

    private let send: Send
    @ObservationIgnored private var chat: ScaenaChat?
    @ObservationIgnored private var asking: Task<Void, Never>?
    @ObservationIgnored private var counted = 0

    public init(editor: DeckEditor, send: @escaping Send = Assistant.urlSession) {
        self.editor = editor
        self.send = send
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
                let (data, status) = try await send(request)
                try Task.checkCancellation()
                let body = String(decoding: data, as: UTF8.self)
                let next: Next
                do {
                    next = try chat.answer(status: status, body: body)
                } catch {
                    // A refusal, or an answer that does not read, said in a person's words (PLAN
                    // 3.18), never the provider's own: the conversation is as it was.
                    add(.failed(Why.answered(chat.provider, status: status, body: body)))
                    return
                }
                switch next {
                case .done(let text, let stop, let used):
                    count(used)
                    if !text.isEmpty { add(.said(text)) }
                    add(.done(stop))
                    return
                case .calls(let text, let calls, let used):
                    count(used)
                    if !text.isEmpty { add(.said(text)) }
                    for (index, call) in calls.enumerated() {
                        // A call the user stopped before it ran is answered as stopped.
                        if Task.isCancelled { break }
                        let at = add(.call(name: call.name, args: call.args, ran: nil))
                        let ran = try chat.run(index, on: editor.session)
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

    /// A question stopped: every call of the last answer answered, as stopped where it never ran.
    private func settle(_ chat: ScaenaChat) {
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
        "data_attach": ("Adding data…", "Added data", "Didn't add data"),
        "data_edit": ("Changing the data…", "Changed the data", "Didn't change the data"),
        "deck_create": ("Making a deck…", "Made a deck", "Didn't make a deck"),
        "deck_diff": ("Comparing versions…", "Compared versions", "Didn't compare versions"),
        "deck_export": ("Exporting the deck…", "Exported the deck", "Didn't export the deck"),
        "deck_find": ("Searching the deck…", "Searched the deck", "Didn't search the deck"),
        "deck_haiku": ("Writing a haiku…", "Put a haiku on a slide", "Didn't write a haiku"),
        "deck_health": ("Reading the deck's story…", "Read the deck's story", "Didn't read the deck's story"),
        "deck_history": ("Reading the history…", "Looked at the history", "Didn't look at the history"),
        "deck_inspect": ("Looking over the deck…", "Looked over the deck", "Didn't look over the deck"),
        "deck_lint": ("Checking the deck…", "Checked the deck", "Didn't check the deck"),
        "deck_patch": ("Editing the deck…", "Edited the deck", "Didn't edit the deck"),
        "deck_read": ("Reading the deck…", "Went through the deck", "Didn't go through the deck"),
        "deck_render": ("Looking at a slide…", "Looked at a slide", "Didn't look at a slide"),
        "icon_find": ("Finding an icon…", "Found an icon", "Didn't find an icon"),
        "map_fetch": ("Fetching a map…", "Put a map on the slide", "Didn't fetch a map"),
        "spine_read": ("Reading the outline…", "Went through the outline", "Didn't go through the outline"),
        "spine_update": ("Changing the outline…", "Changed the outline", "Didn't change the outline"),
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
