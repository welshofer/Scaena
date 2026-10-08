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
            add(.failed("\(error)"))
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
                switch try chat.answer(status: status, body: String(decoding: data, as: UTF8.self)) {
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
            if Task.isCancelled { settle(chat) } else { add(.failed("\(error)")) }
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
