import Foundation
import ScaenaKit
import Testing

/// An answer of Anthropic's that says `text` and calls `calls`, each `(id, name, input as JSON)`.
private func calling(_ text: String, _ calls: [(String, String, String)]) -> String {
    let uses = calls.map { id, name, input in
        #"{"type": "tool_use", "id": "\#(id)", "name": "\#(name)", "input": \#(input)}"#
    }
    let content = ([#"{"type": "text", "text": "\#(text)"}"#] + uses).joined(separator: ", ")
    return #"{"content": [\#(content)], "stop_reason": "tool_use"}"#
}

/// An answer of Anthropic's that says `text` and is done.
private func answering(_ text: String) -> String {
    #"{"content": [{"type": "text", "text": "\#(text)"}], "stop_reason": "end_turn", "usage": {"input_tokens": 10, "output_tokens": 2}}"#
}

/// The conversation through the C ABI (PLAN 3.6, ADR-0022): the question begun with what the
/// window shows, the request built with the key, each call the model makes run on B1 as the
/// browser runs it, the results handed back, and a refusal thrown.
@Test func aConversationRunsEachCallOnTheBundleAsTheBrowsersDoes() throws {
    let session = try ScaenaSession(directory: b1)
    let chat = try ScaenaChat(provider: .anthropic, model: "a-model")
    let seeing = Seeing(state: "cover", nodes: [.init(node: "title", type: "text")])
    try chat.ask("Is the cover clean?", seeing: seeing, on: session)

    let request = try chat.request(key: "sk-test")
    #expect(request.url == "https://api.anthropic.com/v1/messages")
    let made = try request.urlRequest()
    #expect(made.httpMethod == "POST")
    #expect(made.value(forHTTPHeaderField: "x-api-key") == "sk-test")
    let sent = try JSONDecoder().decode(JSONValue.self, from: try #require(made.httpBody))
    #expect(sent["system"]?[0]?["text"]?.string?.hasPrefix("You are the assistant in Scaena's Mac app") == true)
    let question = sent["messages"]?[0]?["content"]?[0]?["text"]?.string
    #expect(question == "[In the window: state cover shown; selected: title (text).]\n\nIs the cover clean?")

    let answer = calling(
        "Looking.", [("t1", "deck_lint", #"{"state": "cover"}"#), ("t2", "deck_render", #"{"state": "cover"}"#)])
    guard case .calls(let text, let calls, _) = try chat.answer(status: 200, body: answer) else {
        Issue.record("the model made calls")
        return
    }
    #expect(text == "Looking.")
    #expect(calls.map(\.name) == ["deck_lint", "deck_render"])
    let lint = try chat.run(0, on: session)
    #expect(!lint.error && !lint.edited)
    let render = try chat.run(1, on: session)
    let frame = try #require(render.frame)
    #expect(frame.starts(with: [0x89, 0x50, 0x4E, 0x47]), "deck_render's frame, a PNG")
    #expect(try chat.next(), "a round taken, of 32")

    guard case .done(let said, let stop, let usage) = try chat.answer(status: 200, body: answering("Clean.")) else {
        Issue.record("the model answered")
        return
    }
    #expect(said == "Clean." && stop == "end")
    #expect(usage == Usage(input: 10, output: 2))
    #expect(throws: ScaenaError.self) {
        try chat.answer(status: 401, body: #"{"error": {"message": "invalid x-api-key"}}"#)
    }
    chat.forget()
    #expect(try chat.conversation() == .array([]))
}

/// The models a key can use: the request, and the list its answer gives.
@Test func aKeyListsItsModels() throws {
    let request = try Provider.openai.modelsRequest(key: "sk-test")
    #expect(request.url == "https://api.openai.com/v1/models")
    #expect(request.headers["authorization"] == "Bearer sk-test")
    #expect(try Provider.openai.models(status: 200, body: #"{"data": [{"id": "b"}, {"id": "a"}]}"#) == ["a", "b"])
    #expect(throws: ScaenaError.self) { try Provider.gemini.models(status: 403, body: "{}") }
}

/// What a stand-in for the network answers, in order, and the requests it was asked.
private actor Answers {
    private var bodies: [String]
    private(set) var asked: [URLRequest] = []

    init(_ bodies: [String]) {
        self.bodies = bodies
    }

    func answer(_ request: URLRequest) -> (Data, Int) {
        asked.append(request)
        guard !bodies.isEmpty else { return (Data(#"{"error": {"message": "no more"}}"#.utf8), 500) }
        return (Data(bodies.removeFirst().utf8), 200)
    }
}

/// The assistant's loop (PLAN 3.6): the question asked, the edit the model calls made on the deck
/// the window shows and handed to the window's undo, and the answer; the conversation shows each.
@MainActor
@Test func theAssistantMakesTheModelsEditsAsTheWindowsOwn() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let patch = #"{"ops": [{"op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Loudly: "}]}"#
    let answers = Answers([calling("Making it louder.", [("t1", "deck_patch", patch)]), answering("Done.")])
    let assistant = Assistant(editor: editor) { request in await answers.answer(request) }
    var undone: [String] = []
    assistant.edited = { before, _ in undone.append(before) }
    let was = editor.source

    assistant.ask(
        "Say it louder.", provider: .anthropic, model: "a-model", key: "sk-test", seeing: Seeing(state: "cover"))
    #expect(assistant.working)
    for _ in 0..<500 where assistant.working {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(!assistant.working, "the question was answered")
    #expect(editor.source.contains("Loudly: "), "the window's deck took the edit")
    #expect(undone == [was], "the window's undo was handed the source it replaced")
    #expect(await answers.asked.count == 2)
    #expect(await answers.asked.first?.value(forHTTPHeaderField: "x-api-key") == "sk-test")

    let shown = assistant.entries.map { entry -> String in
        switch entry.kind {
        case .question(let text): "asked \(text)"
        case .said(let text): "said \(text)"
        case .call(let name, _, let ran): "\(name): \(ran?.summary ?? "running")"
        case .done(let why): "done: \(why)"
        case .failed(let why): "failed: \(why)"
        }
    }
    try #require(shown.count == 5, "\(shown)")
    #expect(shown[0] == "asked Say it louder.")
    #expect(shown[1] == "said Making it louder.")
    #expect(shown[2].hasPrefix("deck_patch: applied"), "\(shown[2])")
    #expect(shown[3...] == ["said Done.", "done: end"])
    #expect(assistant.usage == Usage(input: 10, output: 2))

    // A refusal is said, and the conversation goes on from where it was.
    assistant.ask("Again?", provider: .anthropic, model: "a-model", key: "sk-test")
    for _ in 0..<500 where assistant.working {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard case .failed(let why) = assistant.entries.last?.kind else {
        Issue.record("the refusal is said")
        return
    }
    #expect(why.contains("500"), "\(why)")
}

/// Whether this Mac lets a test write the Keychain: a runner without a login keychain does not.
private func keychainWritable() -> Bool {
    let probe = Keychain(service: "com.scaena.tests.probe")
    guard (try? probe.set("probe", for: .anthropic)) != nil else { return false }
    try? probe.set(nil, for: .anthropic)
    return true
}

/// A key is kept in the Keychain under the app's service, one for each provider, replaced, and
/// taken away (ADR-0006).
@Test(.enabled(if: keychainWritable())) func aKeyIsKeptInTheKeychainAndTakenAway() throws {
    let keychain = Keychain(service: "com.scaena.tests.\(UUID().uuidString)")
    #expect(keychain.key(for: .openai) == nil)
    try keychain.set("sk-one", for: .openai)
    #expect(keychain.key(for: .openai) == "sk-one")
    try keychain.set("sk-two", for: .openai)
    #expect(keychain.key(for: .openai) == "sk-two")
    #expect(keychain.providers == [.openai])
    try keychain.set(nil, for: .openai)
    #expect(keychain.key(for: .openai) == nil)
    #expect(keychain.providers.isEmpty)
}
