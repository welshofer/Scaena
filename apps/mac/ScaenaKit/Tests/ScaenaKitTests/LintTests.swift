import Foundation
@testable import ScaenaKit
import Testing

/// The window answers while the deck is linted (PLAN 4.13): B1 opens, its slides drawn and
/// edited, before its every state is linted; that lint is laid out away from the caller, and its
/// findings come after, where nothing was compiled since.
@Test func b1OpensBeforeItsEveryStateIsLintedAndTheFindingsComeAfter() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    #expect(editor.valid && editor.slots.count == 40)
    #expect(!editor.whole && !editor.linting, "opened, its every state not yet linted")
    #expect(editor.drawing("cover", width: 160) != nil, "the slides are drawn meanwhile")

    // The headline on another slide in the color of what lies behind it: the state shown is
    // linted at once, and that one waits for every state's lint.
    editor.shown = "cover"
    let faint: JSONValue = [
        "op": "choose", "node": "headline", "prop": "style/color", "value": "surface", "state": "goal",
    ]
    try editor.make([faint])
    let unread = { (editor: DeckEditor) in
        editor.findings.contains { $0.state == "goal" && $0.node == "headline" && ["E110", "E111"].contains($0.code) }
    }
    #expect(!editor.whole && !unread(editor), "the state shown alone is linted at an edit")

    await editor.lintEvery()
    #expect(editor.whole && !editor.linting)
    #expect(unread(editor), "every state linted: \(editor.findings.map(\.code))")

    // A lint of one state after it keeps what it found on the others.
    editor.lintShown()
    #expect(!editor.whole && unread(editor))
}

/// A deck changed while its every state is being linted (PLAN 4.13): what lint found in the deck
/// as it was is dropped, never replacing what it found in the deck as it is; the next lint of
/// every state answers for it.
@Test @MainActor func aLintOfADeckChangedSinceItBeganIsDropped() async throws {
    let window = Window(editor: DeckEditor(session: try ScaenaSession(directory: b1)))
    window.editor.shown = "cover"
    let typed: JSONValue = [
        "op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Edited. ",
    ]
    // The edit is made once the lint has taken the deck and counted itself, on the window's
    // actor, before it waits for the layout: under way, with no flag polled to catch it by.
    var edited: Result<String, any Error>?
    window.editor.begun = {
        #expect(window.editor.linting, "every state being linted")
        edited = Result { try window.editor.make([typed]) }
    }
    await window.editor.lintEvery()
    window.editor.begun = nil
    _ = try #require(edited, "the lint of every state began").get()
    #expect(!window.editor.whole && !window.editor.linting, "the lint of the deck before the edit is dropped")

    await window.editor.lintEvery()
    #expect(window.editor.whole, "the deck as it is, linted")

    // Taken from the session, laid out, and handed back after another source compiled: none.
    let session = window.editor.session
    let laying = try #require(session.laying())
    laying.lay()
    #expect(try session.lint(laid: laying)?.whole == true)
    let stale = try #require(session.laying())
    stale.lay()
    _ = try window.editor.make([typed])
    #expect(try session.lint(laid: stale) == nil, "another source compiled since")
}

/// The window's editor, on the main actor, as the window holds it.
@MainActor private final class Window {
    let editor: DeckEditor

    init(editor: DeckEditor) {
        self.editor = editor
    }
}
