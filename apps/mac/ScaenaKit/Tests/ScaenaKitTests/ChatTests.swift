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

/// Anthropic's answer `text`, as it streams it: each word its own event, so a window shows them
/// as they are written (narrative ATTR-3).
private func streaming(_ text: String) -> [String] {
    var events = [
        #"event: message_start\#ndata: {"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "usage": {"input_tokens": 10, "output_tokens": 0}}}\#n\#n"#,
        #"event: content_block_start\#ndata: {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}\#n\#n"#,
    ]
    for word in text.split(separator: " ", omittingEmptySubsequences: false).enumerated() {
        let piece = (word.offset == 0 ? "" : " ") + word.element
        events.append(#"event: content_block_delta\#ndata: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "\#(piece)"}}\#n\#n"#)
    }
    events += [
        #"event: content_block_stop\#ndata: {"type": "content_block_stop", "index": 0}\#n\#n"#,
        #"event: message_delta\#ndata: {"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 2}}\#n\#n"#,
        #"event: message_stop\#ndata: {"type": "message_stop"}\#n\#n"#,
    ]
    return events
}

/// An answer streamed is read as one sent whole (narrative ATTR-3): the words each piece
/// completes come out as it is fed, and `answer` reads the body assembled.
@Test func aStreamedAnswerReadsAsOneSentWhole() throws {
    let session = try ScaenaSession(directory: b1)
    let chat = try ScaenaChat(provider: .anthropic, model: "a-model")
    try chat.ask("Is it clean?", on: session)
    _ = try chat.request(key: "sk-test")
    let events = streaming("The cover is clean.")
    var heard: [String] = []
    // Fed in pieces that cut events mid-way: a line, then the rest.
    let body = events.joined()
    let cut = body.index(body.startIndex, offsetBy: 200)
    heard.append(try chat.heard(String(body[..<cut])))
    heard.append(try chat.heard(String(body[cut...])))
    #expect(heard.joined() == "The cover is clean.", "\(heard)")
    guard case .done(let said, let stop, let usage) = try chat.answer(status: 200, body: body) else {
        Issue.record("the model answered")
        return
    }
    #expect(said == "The cover is clean." && stop == "end")
    #expect(usage == Usage(input: 10, output: 2))
}

/// The assistant shows the answer's words as they arrive (narrative ATTR-3): with the stream
/// held after its first words, the conversation shows them before the answer is whole, and the
/// whole answer stands in their place at the end.
@MainActor
@Test func theAssistantShowsTheAnswerAsItIsWritten() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let events = streaming("The cover is clean.")
    let seen = Seen()
    let assistant = Assistant(editor: editor, send: { _ in (Data(), 0) }) { _, heard in
        var whole = ""
        for (i, event) in events.enumerated() {
            whole += event
            await heard(event)
            // After the first word, what the window shows meanwhile.
            if i == 2 { await seen.keep(await MainActor.run { said(assistant) }) }
        }
        return (Data(whole.utf8), 200)
    }
    assistant.ask("Is the cover clean?", provider: .anthropic, model: "a-model", key: "sk-test")
    for _ in 0..<500 where assistant.working {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(!assistant.working)
    #expect(await seen.kept == ["The"], "the first word shows before the rest is written")
    #expect(said(assistant) == ["The cover is clean."], "the answer whole, in its place")
    #expect(assistant.usage == Usage(input: 10, output: 2))
}

/// What the conversation shows the model as having said.
@MainActor
private func said(_ assistant: Assistant) -> [String] {
    assistant.entries.compactMap { entry in
        if case .said(let text) = entry.kind { text } else { nil }
    }
}

/// What a test saw at one moment.
private actor Seen {
    private(set) var kept: [String] = []

    func keep(_ seen: [String]) {
        kept = seen
    }
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
    #expect(shown[2].hasPrefix("deck_patch: Done"), "\(shown[2])")
    #expect(shown[3...] == ["said Done.", "done: end"])
    #expect(assistant.usage == Usage(input: 10, output: 2))

    // A refusal is said in a person's words (PLAN 3.18), not the provider's or its status, and
    // the conversation goes on from where it was.
    assistant.ask("Again?", provider: .anthropic, model: "a-model", key: "sk-test")
    for _ in 0..<500 where assistant.working {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard case .failed(let why) = assistant.entries.last?.kind else {
        Issue.record("the refusal is said")
        return
    }
    #expect(why == "Anthropic couldn't answer just now. Try again in a moment.", "\(why)")
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

/// Each tool the MCP server serves (`docs/schema/mcp`) has a person's words for the window (PLAN
/// 3.18): what the assistant is doing while it runs, and what it did. So no tool's name reaches the
/// window, and a tool added without words fails here.
@Test func everyToolIsNamedInAPersonsWords() throws {
    let folder = repository.appending(path: "docs/schema/mcp")
    let tools = try FileManager.default.contentsOfDirectory(atPath: folder.path(percentEncoded: false))
        .filter { $0.hasSuffix(".json") }
        .map { String($0.dropLast(".json".count)) }
    #expect(tools.count >= 16)
    for tool in tools {
        #expect(ToolWords.knows(tool), "\(tool) has no words of its own")
        #expect(!ToolWords.doing(tool).contains("_") && !ToolWords.done(tool).contains("_"), "\(tool)")
    }
    #expect(ToolWords.doing("deck_patch") == "Editing the deck…")
    #expect(ToolWords.done("deck_patch") == "Edited the deck")
    #expect(ToolWords.doing("a_tool_to_come") == "Working…")
    for tool in tools { #expect(!ToolWords.undone(tool).contains("_"), "\(tool)") }
}

/// A call refused is headed as not done (PLAN 2.99), as the browser heads it: a patch that would
/// leave the deck invalid edits nothing, and the window does not say it edited the deck.
@Test func aRefusedCallIsHeadedAsNotDone() throws {
    let session = try ScaenaSession(directory: b1)
    let chat = try ScaenaChat(provider: .anthropic, model: "a-model")
    try chat.ask("Lay the cover out anew.", seeing: Seeing(state: "cover", nodes: []), on: session)
    let patch = #"{"ops": [{"op": "set_state", "id": "cover", "prop": "layout", "value": "no-such-layout"}]}"#
    guard case .calls = try chat.answer(status: 200, body: calling("Trying.", [("t1", "deck_patch", patch)])) else {
        Issue.record("the model made a call")
        return
    }
    let ran = try chat.run(0, on: session)
    #expect(!ran.made && !ran.edited, "\(ran.summary)")
    #expect(ToolWords.ran(ran) == "Didn't edit the deck")
}

/// The writing tools (PLAN 2.101): what a model is asked for a text and for a slide, from the deck
/// as it stands, in the engine's words; the request that asks it once, with nothing to call; and
/// the patch that takes its answer, as the browser takes one.
@Test func aWritingToolAsksOnceAndTakesTheAnswerAsOnePatch() throws {
    let session = try ScaenaSession(directory: b1)
    let ask = WritingAsk(kind: .tighten, state: "cover", node: "subtitle")
    let writing = try session.writing(ask)
    let words = try #require(writing.task.text)
    #expect(words.hasPrefix("Why this exists"))
    #expect(writing.title == "Tightened" && writing.prompt == words)
    #expect(writing.instructions.hasPrefix("You tighten the words of one text on a slide"))
    let notes = try session.writing(WritingAsk(kind: .notes, state: "cover"))
    #expect(notes.prompt.hasPrefix("The slide reads:\nTitle: Scaena"))

    let request = try Provider.anthropic.onceRequest(
        model: "a-model", key: "sk-test", instructions: writing.instructions, prompt: writing.prompt)
    let sent = try JSONDecoder().decode(JSONValue.self, from: try #require(try request.urlRequest().httpBody))
    #expect(sent["tools"] == nil, "a question asked once sends no tools")
    #expect(sent["system"]?[0]?["text"]?.string == writing.instructions)
    #expect(sent["messages"]?[0]?["content"]?[0]?["text"]?.string == words)
    let answer = try Provider.anthropic.answered(status: 200, body: answering("“Why Scaena exists.”"))
    #expect(answer == "“Why Scaena exists.”")

    let written = try session.written(ask, answer: answer)
    #expect(written.text == "Why Scaena exists.", "the quotes a model wraps it in come off")
    let patch = try #require(written.patch)
    #expect(!patch.isEmpty && patch.allSatisfy { $0["op"]?.string == "replace_text" })
    let drafted = try session.written(WritingAsk(kind: .notes, state: "cover"), answer: "Open on the name.")
    #expect(drafted.patch?.first?["op"]?.string == "set_state")

    // Use it on an answer that changes nothing, or on no answer, takes nothing, which the window
    // says, rather than a change that failed.
    #expect(try session.taking(ask, answer: words) == nil, "the words it has already")
    #expect(try session.taking(ask, answer: "  ") == nil, "no answer")
    #expect(try session.taking(ask, answer: answer)?.isEmpty == false)
}

/// The inspector's writing tools are offered for what has words to write (PLAN 2.101), as the
/// browser offers them: a text, a picture, or the slide; a shape has none.
@Test func writingIsOfferedForATextAPictureOrTheSlide() throws {
    let session = try ScaenaSession(directory: tortureDeck)
    #expect(try session.choices(state: "shapes", node: "case").writes)
    #expect(try !session.choices(state: "shapes", node: "shape-panel").writes)
    #expect(try session.choices(state: "adjust", node: "adj-mono").writes)
    #expect(try session.stateChoices(state: "shapes").writes)
}

/// An issue is explained as the window says it (PLAN 2.99, 2.101), as the browser asks: the model
/// is asked the issue's sentence, not lint's own message.
@Test func anIssueIsExplainedAsTheWindowSaysIt() throws {
    let session = try ScaenaSession(directory: b1)
    let compiled = try session.compile(try session.source() + "\nthis is not scn {")
    let issue = try #require(compiled.error)
    let said = try #require(issue.said)
    let writing = try session.writing(WritingAsk(kind: .explain, state: "cover", finding: issue))
    #expect(writing.prompt.contains(said), "\(writing.prompt)")
    #expect(!writing.prompt.contains(issue.message), "\(writing.prompt)")
}

/// Describe the Picture (PLAN 2.119): the picture as the slide shows it goes with the question, a
/// PNG shown before its words, as the browser sends one; and the answer is the picture's
/// description, one `choose` of `alt`.
@Test func aPictureIsDescribedFromItselfAsTheSlideShowsIt() throws {
    let session = try ScaenaSession(directory: tortureDeck)
    let ask = WritingAsk(kind: .describe, state: "adjust", node: "adj-mono")
    let writing = try session.writing(ask)
    #expect(writing.title == "Description")
    #expect(writing.task.text == "The same picture in black and white.")
    let picture = try #require(writing.picture)
    #expect(Data(base64Encoded: picture)?.starts(with: [0x89, 0x50, 0x4E, 0x47]) == true, "a PNG")

    let request = try Provider.anthropic.onceRequest(
        model: "a-model", key: "sk-test", instructions: writing.instructions, prompt: writing.prompt,
        picture: picture)
    let sent = try JSONDecoder().decode(JSONValue.self, from: try #require(try request.urlRequest().httpBody))
    let content = sent["messages"]?[0]?["content"]
    #expect(content?[0]?["type"]?.string == "image" && content?[0]?["source"]?["data"]?.string == picture)
    #expect(content?[1]?["text"]?.string == writing.prompt)

    let written = try session.written(ask, answer: "“A gray gradient, a dark block, and a lighter disc.”")
    #expect(written.text == "A gray gradient, a dark block, and a lighter disc.")
    let op = try #require(written.patch?.first)
    #expect(op["op"]?.string == "choose" && op["prop"]?.string == "alt" && op["node"]?.string == "adj-mono")
}
