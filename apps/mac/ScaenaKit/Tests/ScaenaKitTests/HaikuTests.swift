import Foundation
import Testing

@testable import ScaenaKit

/// What a test's provider was asked, request by request.
private actor Asked {
    private(set) var prompts: [String] = []

    func add(_ body: String) {
        prompts.append(body)
    }
}

private let near = "An old silent pond\nA green frog jumps into the pond\nSplash! Silence again"
private let kept = "An old silent pond\nA frog jumps into the pond\nSplash! Silence again"

/// Haiku… (PLAN O.4g), as the Mac's and the iPad's sheet writes one, against a stand-in for
/// Anthropic's Messages API made through `scaena_providers` as `URLSession` makes it: an answer of 5,
/// 8, and 5 is asked once more with each line's count, and the second, of 5, 7, and 5, offered;
/// Use It puts it on a new Haiku slide after the slide shown, one patch, which one undo takes
/// back; the slide's notes name the model and the provider, and nothing on the slide does.
@MainActor
@Test func aNearMissIsAskedOnceMoreAndTheSecondPlacedInOnePatch() async throws {
    let asked = Asked()
    let send: Assistant.Send = { request in
        let body = String(decoding: request.httpBody ?? Data(), as: UTF8.self)
        await asked.add(body)
        let answer = await asked.prompts.count == 1 ? near : kept
        let reply = JSONValue.object([
            "content": .array([.object(["type": "text", "text": .string(answer)])]), "stop_reason": "end_turn",
        ])
        return (try JSONEncoder().encode(reply), 200)
    }
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Field notes"))
    let session = editor.session
    let first = try #require(try session.states().first)
    let states = try session.states().count

    // Nothing selected and no verse waiting: the sheet asks where it goes.
    let target = try session.haikuTarget(state: first, selected: [], pressed: nil)
    #expect(target.asksWhere && target.node == nil && !target.waiting)

    let ask = HaikuAsk(state: first, about: .topic, topic: "a field at dawn")
    let (poem, _) = try await Haiku.written(ask, of: session) { instructions, prompt in
        try await Provider.anthropic.write(
            model: "test-model-1", key: "sk-test", base: "http://127.0.0.1:9", instructions: instructions,
            prompt: prompt, send: send)
    }
    let prompts = await asked.prompts
    #expect(prompts.count == 2, "a near miss is asked once more")
    #expect(prompts.first?.contains("Write a haiku about: a field at dawn") == true)
    #expect(prompts.last?.contains("Its lines count 5, 8, and 5 syllables") == true)
    #expect(poem.lines == kept.components(separatedBy: "\n"))
    #expect(poem.checked.kept == "kept")
    #expect(poem.checked.lines.map { $0.count?.said } == ["5", "7", "5"])

    // An edit counted again, a line off its count marked.
    let edited = try session.verseCounts([poem.lines[0], "A green frog jumps into the pond", poem.lines[2]])
    #expect(edited.lines[1].count?.said == "8" && edited.lines[1].off && !edited.lines[0].off)

    // Use It: one patch, a new Haiku slide after the slide shown.
    let taken = try session.haikuTaken(
        ask, lines: poem.lines, to: .new, by: HaikuWriter(model: "test-model-1", provider: "Anthropic"),
        date: "October 15, 2026")
    guard case .offer(_, _, let placed?) = taken else {
        Issue.record("nothing placed: \(taken)")
        return
    }
    let before = try editor.make(placed.patch)
    #expect(try session.states().count == states + 1)
    #expect(try session.states().firstIndex(of: placed.state) == 1, "after the slide shown")
    let source = editor.source
    #expect(source.contains("layout:haiku"))
    #expect(source.contains("Haiku written by test-model-1 with your Anthropic key on October 15, 2026, about a field at dawn."))
    #expect(source.components(separatedBy: "test-model-1").count == 2, "the model is named in the notes alone")
    #expect(!source.contains("Who wrote it"), "the attribution waits empty, never filled in")

    // One undo takes it back.
    _ = editor.restore(before)
    #expect(try session.states().count == states)

    // Placed again, its verse selected: it is not asked where the poem goes, and Write Another's
    // poem goes over it.
    try editor.make(placed.patch)
    let over = try session.haikuTarget(state: placed.state, selected: [placed.node], pressed: nil)
    #expect(over.node == placed.node && !over.asksWhere)
}

/// The on-device model, stubbed: a poem kept to 5-7-5 the first time is offered with no second
/// ask, and its slide's notes name the model on this device, with no key.
@MainActor
@Test func theModelOnTheDeviceIsAskedOnceAndNamedWithNoKey() async throws {
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Gallery", title: "Field notes"))
    let session = editor.session
    let first = try #require(try session.states().first)
    var asks = 0
    let ask = HaikuAsk(state: first, about: .talk)
    let (poem, note) = try await Haiku.written(ask, of: session) { instructions, prompt in
        asks += 1
        #expect(instructions.contains("haiku"))
        #expect(prompt.contains("<talk>"), "the whole talk inside <talk> tags")
        return kept
    }
    #expect(asks == 1 && note == nil && poem.checked.kept == "kept")
    let taken = try session.haikuTaken(
        ask, lines: poem.lines, to: .new, attribution: "after a long day",
        by: HaikuWriter(model: "the model on this Mac"), date: Haiku.today(Date(timeIntervalSince1970: 1792065600)))
    guard case .offer(_, _, let placed?) = taken else {
        Issue.record("nothing placed: \(taken)")
        return
    }
    try editor.make(placed.patch)
    #expect(editor.source.contains("Haiku written by the model on this Mac on October 15, 2026, about the whole talk."))
    #expect(editor.source.contains("after a long day"))
    #expect(!editor.source.contains(" key on "), "no key named")
}
