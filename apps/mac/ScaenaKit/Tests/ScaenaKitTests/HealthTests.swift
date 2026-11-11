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

/// While the source does not compile, the Story panel keeps its last read, said to be so, its
/// fixes held (lift-up 20261110 deferred, as the browser's Story tab): no story of the deck before
/// the error stands as this one's, and the source mended, the story is read again as it stands.
@Test @MainActor func theStoryIsKeptAsLastReadWhileTheSourceHasAnError() async throws {
    let window = HealthWindow(editor: DeckEditor(session: try ScaenaSession(directory: b1)))
    let editor = window.editor
    let fresh = try #require(try await editor.story(after: nil))
    #expect(!fresh.asLastRead)
    #expect(fresh.listed("What to Look At") == "What to Look At")

    let good = editor.source
    _ = editor.type(good + "\n}}} not a deck")
    #expect(!editor.compiles, "the source still compiles")
    let kept = try #require(try await editor.story(after: fresh.health))
    #expect(kept.asLastRead, "a story read while the source has an error is shown as current")
    #expect(kept.health.score == fresh.health.score)
    #expect(kept.listed("What to Look At") == "What to Look At, as last read")
    #expect(StoryRead.said == "Showing the story as last read: the source has an error to fix")

    _ = editor.type(good)
    #expect(editor.compiles)
    let again = try #require(try await editor.story(after: kept.health))
    #expect(!again.asLastRead, "the mended source's story is still shown as last read")
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

/// A slide that holds balance down ranks by balance's score, as the browser's Story ranks it,
/// and folds as a thin slide (R1's MCP dogfood): a thin warning under balance at 60 opens ahead
/// of a tone warning at 72, and folded it is said in words.
@Test func aThinSlideRanksByBalanceAndFoldsInWords() throws {
    let listed = [(3, "tone"), (4, "thin")].map { slide, kind in
        #"{"state": "s\#(slide)", "slide": \#(slide), "severity": "warning", "kind": "\#(kind)", "says": "", "ask": ""}"#
    }
    let measure = { (score: Int?) in #"{"score": \#(score.map(String.init) ?? "null"), "says": ""}"# }
    let json = """
        {"score": 80, "arc": null, "pacing": \(measure(90)), "balance": \(measure(60)), "coherence": \(measure(nil)),
         "fidelity": \(measure(85)), "intensity": \(measure(80)), "tone": \(measure(72)),
         "slides": [], "findings": [\(listed.joined(separator: ","))]}
        """
    let health = try JSONDecoder().decode(Health.self, from: Data(json.utf8))
    #expect(health.triaged(open: 1).open.map(\.kind) == ["thin"], "balance at 60 before tone at 72")
    #expect(Health.moreSaid(health.triaged(open: 1).more) == "1 more: 1 tone")
    #expect(Health.moreSaid(health.triaged(open: 0).more) == "2 more: 1 tone, 1 thin slide")
}

/// A figure no source backs cites one of the deck's sources (W1's dogfood 5): `source:NAME` added to
/// its beat's evidence where `StateChoices.cites` says it lives, the list whole where the beat cites
/// nothing yet, and nothing made where it cites that source already. A deck made by hand was
/// grounded in nothing, and its states' choices say where their beat's evidence is.
@Test @MainActor func aFigureCitesASourceInItsBeatsEvidence() throws {
    func choice(_ json: String) throws -> CiteChoice { try JSONDecoder().decode(CiteChoice.self, from: Data(json.utf8)) }
    let none = try choice(#"{"beat": "opening", "at": "/spine/sections/0/beats/0/evidence", "evidence": []}"#)
    #expect(
        none.citing("notes.md")
            == [["op": "add", "path": "/spine/sections/0/beats/0/evidence", "value": ["source:notes.md"]]])
    let some = try choice(#"{"beat": "doubled", "at": "/spine/sections/1/beats/0/evidence", "evidence": ["@q3"]}"#)
    #expect(some.citing("notes.md") == [["op": "add", "path": "/spine/sections/1/beats/0/evidence/-", "value": "source:notes.md"]])
    let cited = try choice(#"{"beat": "doubled", "at": "/x", "evidence": ["@q3", "source:notes.md"]}"#)
    #expect(cited.citing("notes.md") == nil, "a source cited twice")

    let sources = try JSONDecoder().decode(
        [Grounded].self,
        from: Data(#"[{"name": "notes.md", "kind": "file", "bytes": 10, "truncated": false, "cited": [], "states": []}]"#.utf8))
    #expect(sources == [Grounded(name: "notes.md", kind: "file", url: nil)])

    let session = try ScaenaSession(directory: b1)
    #expect(try session.grounded().isEmpty, "B1, made by hand, was grounded in something")
    let state = try #require(try session.states().first)
    _ = try session.stateChoices(state: state).cites
}
