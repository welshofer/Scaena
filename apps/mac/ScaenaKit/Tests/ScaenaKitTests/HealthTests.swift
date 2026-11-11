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

/// What holds the score down is said beside it, as the browser's Story says it (W1's dogfood 5):
/// the slides lint finds an error on, each with the state that shows it, the cap they set, and the
/// score without it; nothing where nothing caps it.
@Test func aCappedScoreSaysWhichSlidesHoldItDown() throws {
    let measure = #"{"score": 90, "says": ""}"#
    let read = { (n: Int) in
        #"{"state": "s\#(n)", "part": null, "title": "", "curve": 0.5, "declared": null, "read": null, "tone": null, "toneRead": null, "standing": "ok"}"#
    }
    let health = { (capped: String) throws -> Health in
        let json = """
            {"score": 81, "arc": null, "pacing": \(measure), "balance": \(measure), "coherence": \(measure),
             "fidelity": \(measure), "intensity": \(measure), "tone": \(measure),
             "slides": [\((1...4).map(read).joined(separator: ","))], "findings": []\(capped)}
            """
        return try JSONDecoder().decode(Health.self, from: Data(json.utf8))
    }
    let one = try health(#", "capped": {"slides": [2], "cap": 81, "score": 94}"#)
    #expect(one.capped?.slides == [2] && one.capped?.cap == 81 && one.capped?.score == 94)
    #expect(one.cappedSaid == "Capped at 81 while slide 2 has an error; 94 without.")
    #expect(one.cappedParts.compactMap { $0.state } == ["s2"], "the slide a button that shows it")
    let two = try health(#", "capped": {"slides": [1, 3], "cap": 62, "score": 90}"#)
    #expect(two.cappedSaid == "Capped at 62 while slide 1 and slide 3 have an error; 90 without.")
    let three = try health(#", "capped": {"slides": [1, 3, 9], "cap": 42, "score": 88}"#)
    #expect(three.cappedSaid == "Capped at 42 while slide 1, slide 3 and slide 9 have an error; 88 without.")
    #expect(three.cappedParts.compactMap { $0.state } == ["s1", "s3"], "a slide health does not read is said, no button")
    let none = try health("")
    #expect(none.capped == nil && none.cappedSaid == nil && none.cappedParts.isEmpty)
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

/// A health read of `slides` slides, `s1` … `sN`, with `findings`, each `(slide, severity, kind)`.
private func read(slides: Int, _ findings: [(Int, String, String)]) throws -> Health {
    let measure = #"{"score": 90, "says": ""}"#
    let slide = { (n: Int) in
        #"{"state": "s\#(n)", "part": null, "title": "", "curve": 0.5, "declared": null, "read": null, "tone": null, "toneRead": null, "standing": "ok"}"#
    }
    let listed = findings.map { n, severity, kind in
        #"{"state": "s\#(n)", "slide": \#(n), "severity": "\#(severity)", "kind": "\#(kind)", "says": "", "ask": ""}"#
    }
    let json = """
        {"score": 90, "arc": null, "pacing": \(measure), "balance": \(measure), "coherence": \(measure),
         "fidelity": \(measure), "intensity": \(measure), "tone": \(measure),
         "slides": [\((1...slides).map(slide).joined(separator: ","))], "findings": [\(listed.joined(separator: ","))]}
        """
    return try JSONDecoder().decode(Health.self, from: Data(json.utf8))
}

/// Every slide a Story row's words name links to it, as the browser's Story makes each a button
/// (editor dogfood 6): "slides 1, 5 and 11", "slide 4's", "slides 10–11", the words as they were,
/// and a slide health does not read left as words.
@Test func everySlideARowNamesLinksToIt() throws {
    let health = try read(slides: 11, [])
    let links = { (text: String) -> [String] in
        let said = health.slidesShown(text)
        #expect(String(said.characters) == text, "the words as they were")
        return said.runs.compactMap { run in run.link.map { "\(String(said[run.range].characters)) \($0.absoluteString)" } }
    }
    #expect(links("Deepen the notes of slides 1, 5 and 11, the shortest beats.") == ["1 scaena-state:s1", "5 scaena-state:s5", "11 scaena-state:s11"])
    #expect(links("Its shortest beat, slide 5's, runs 15 seconds; its longest, slide 10's.") == ["5 scaena-state:s5", "10 scaena-state:s10"])
    #expect(links("The close, slides 10–11, never lands it.") == ["10 scaena-state:s10", "11 scaena-state:s11"])
    #expect(links("Give slide 12 a point more.").isEmpty, "a slide health does not read stays words")
    #expect(links("Reads flatter than it was written to be.").isEmpty)
}

/// The Story's open rows keep their places from one read to the next (editor dogfood 6): a row
/// whose one finding of its kind moved slides stays where it stood, and what is new comes after,
/// an error first, so no new row takes the place of the one a person is working.
@Test func theStorysRowsKeepTheirPlacesAsItReadsAgain() throws {
    var places = StoryPlaces()
    let slides = { (runs: [[Health.Finding]]) in runs.map { run in run.map { "\($0.slide) \($0.kind)" }.joined(separator: ", ") } }
    let first = try read(slides: 11, [(8, "warning", "tone"), (10, "warning", "intensity"), (5, "warning", "pacing"), (3, "warning", "turn")])
    let shown = places.ordered(first.findings.map { [$0] }, of: first.findings)
    #expect(slides(shown) == ["8 tone", "10 intensity", "5 pacing", "3 turn"], "a first read as triage gives it")
    // Slide 5's notes deepened: pacing moves to slide 1, and a tone finding new to the Story comes
    // to slide 5, which triage would put first.
    let again = try read(slides: 11, [(5, "warning", "tone"), (8, "warning", "tone"), (10, "warning", "intensity"), (1, "warning", "pacing"), (3, "warning", "turn")])
    let kept = places.ordered(again.findings.map { [$0] }, of: again.findings)
    #expect(slides(kept) == ["8 tone", "10 intensity", "1 pacing", "3 turn", "5 tone"])
    // A new error comes first, the rest where they stood.
    let broken = try read(slides: 11, [(8, "warning", "tone"), (2, "error", "layout"), (10, "warning", "intensity"), (1, "warning", "pacing")])
    let errorFirst = places.ordered(broken.findings.map { [$0] }, of: broken.findings)
    #expect(slides(errorFirst) == ["2 layout", "8 tone", "10 intensity", "1 pacing"])
}
