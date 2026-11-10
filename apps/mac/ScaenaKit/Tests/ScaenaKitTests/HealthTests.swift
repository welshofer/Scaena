import Foundation
@testable import ScaenaKit
import Testing

/// The deck's story is read away from the window (lift-up narrative PERF-1): the Story panel's
/// health is the session's, read from the deck it compiled last on a queue of its own while the
/// window answers, and a read of a deck an edit has changed since it began is dropped, so an
/// older deck's story never stands for this one's.
@Test @MainActor func theDecksHealthIsReadAwayFromTheWindowAndAStaleReadIsDropped() async throws {
    let window = HealthWindow(editor: DeckEditor(session: try ScaenaSession(directory: b1)))
    let editor = window.editor
    #expect(editor.valid)

    // Read away from the window, it is what the session reads on its own thread.
    let session = try editor.session.health()
    let away = try #require(try await editor.health(), "nothing changed while it was read")
    #expect(away.score == session.score && away.arc == session.arc)
    #expect(away.slides == session.slides)
    #expect(away.findings.map(\.says) == session.findings.map(\.says))

    // An edit made once the read has taken the deck, before its answer: the answer is dropped.
    editor.shown = "cover"
    let typed: JSONValue = [
        "op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Edited. ",
    ]
    var edited: Result<String, any Error>?
    editor.reading = { edited = Result { try editor.make([typed]) } }
    let stale = try await editor.health()
    editor.reading = nil
    _ = try #require(edited, "the read began").get()
    #expect(stale == nil, "the story of the deck before the edit is dropped")

    // The next read answers for the deck as it is.
    #expect(try await editor.health() != nil)

    // Read off the main thread, on a stack that holds a deck larger than B1: a dispatch queue's
    // 512 KB worker leaves too little room.
    let (main, stack) = try await editor.offMain.run { (Thread.isMainThread, Thread.current.stackSize) }
    #expect(!main && stack >= OffMain.stack, "read on the main thread or a small stack: \(main), \(stack)")
}

/// The window's editor, on the main actor, as the window holds it.
@MainActor private final class HealthWindow {
    let editor: DeckEditor

    init(editor: DeckEditor) {
        self.editor = editor
    }
}

/// The Story panel leads with what to do first (lift-up S1): every error a slide's lint finds,
/// then the warnings whose measure scores lowest, one no measure speaks to first and an unscored
/// measure's last, five in all; the rest folded, in the slides' order, counted by kind.
@Test func theStoryLeadsWithWhatToDoFirst() throws {
    let findings: [(Int, String, String)] = [
        (1, "info", "arc"), (2, "warning", "tone"), (3, "warning", "intensity"), (4, "warning", "tone"),
        (5, "error", "layout"), (6, "warning", "story"), (7, "warning", "tone"), (8, "warning", "tone"),
        (9, "info", "arc"), (10, "warning", "turn"),
    ]
    let listed = findings.map { slide, severity, kind in
        #"{"state": "s\#(slide)", "slide": \#(slide), "severity": "\#(severity)", "kind": "\#(kind)", "says": "Slide \#(slide).", "ask": "Ask \#(slide)."}"#
    }
    let measure = { (score: Int?) in #"{"score": \#(score.map(String.init) ?? "null"), "says": ""}"# }
    let json = """
        {"score": 89, "arc": null, "pacing": \(measure(90)), "balance": \(measure(95)), "coherence": \(measure(nil)),
         "fidelity": \(measure(85)), "intensity": \(measure(80)), "tone": \(measure(72)),
         "slides": [], "findings": [\(listed.joined(separator: ","))]}
        """
    let health = try JSONDecoder().decode(Health.self, from: Data(json.utf8))
    let triaged = health.triaged()
    #expect(triaged.open.map(\.slide) == [5, 6, 2, 4, 7], "the error, the slide's own warning, then tone's at 72")
    #expect(triaged.more.map(\.slide) == [1, 3, 8, 9, 10], "the rest, in the slides' order")
    #expect(Health.moreSaid(triaged.more) == "5 more: 2 arc, 1 intensity, 1 tone, 1 turn")
    // Every kind is said by a name a person reads (lift-up 10-25b FUNC-4).
    let named = ["look", "label", "look"].map { kind in
        try! JSONDecoder().decode(Health.Finding.self, from: Data(
            #"{"state": "s1", "slide": 1, "severity": "info", "kind": "\#(kind)", "says": "", "ask": ""}"#.utf8))
    }
    #expect(Health.moreSaid(named) == "3 more: 2 change let go, 1 topic headline")
    #expect(health.triaged(open: 0).open.map(\.slide) == [5], "every error stands open")
}

/// The Story is read again once every state is linted, where that lint finds otherwise than the
/// findings the story shown was read beside (lift-up 20261110 deferred, as the browser's Story):
/// a read before it keeps the state shown's findings, and the fixes and Show they offer; a lint
/// that finds the same reads nothing more.
@Test @MainActor func theStoryIsReadAgainWhenEveryStatesLintFindsOtherwise() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.lintShown()
    #expect(editor.linted == nil, "a lint of the state shown alone stands for every state's")
    let readWith = StoryLint.keyed(editor.findings)
    await editor.lintEvery()
    let linted = try #require(editor.linted, "every state linted, and nothing to read beside")
    #expect(linted == StoryLint.keyed(editor.findings))
    #expect(!StoryLint.rereads(linted, readWith: linted), "a lint that finds the same reads again")
    #expect(!StoryLint.rereads(nil, readWith: readWith))

    func finding(_ code: String, fixable: Bool, message: String = "") throws -> Finding {
        let json = #"{"code": "\#(code)", "severity": "error", "message": "\#(message)", "state": "cover", "node": "title", "fixable": \#(fixable), "shown": true}"#
        return try JSONDecoder().decode(Finding.self, from: Data(json.utf8))
    }
    let before = StoryLint.keyed([try finding("E100", fixable: false)])
    #expect(StoryLint.rereads(StoryLint.keyed([try finding("E100", fixable: true)]), readWith: before))
    #expect(StoryLint.rereads(StoryLint.keyed([try finding("E101", fixable: false)]), readWith: before))
    #expect(StoryLint.rereads(StoryLint.keyed([]), readWith: before))
    #expect(!StoryLint.rereads(StoryLint.keyed([try finding("E100", fixable: false, message: "said otherwise")]), readWith: before))
}

/// The folded count names each kind in a person's words, as the browser's Story does: an
/// unsourced figure, a topic headline, a change let go, a broken slide, and the big idea, never
/// the identifier a finding carries.
@Test func theFoldedCountNamesEachKindInWords() throws {
    let kinds = ["source", "source", "label", "look", "layout", "bigIdea", "tone"]
    let listed = kinds.enumerated().map { i, kind in
        #"{"state": "s\#(i)", "slide": \#(i + 1), "severity": "info", "kind": "\#(kind)", "says": "", "ask": ""}"#
    }
    let findings = try JSONDecoder().decode([Health.Finding].self, from: Data("[\(listed.joined(separator: ","))]".utf8))
    #expect(
        Health.moreSaid(findings)
            == "7 more: 2 unsourced figure, 1 topic headline, 1 change let go, 1 broken slide, 1 big idea, 1 tone")
    #expect(Health.kindSaid("arc") == "arc", "a kind with no words of its own is said as it is")
}
