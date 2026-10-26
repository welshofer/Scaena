import Foundation
import ScaenaKit
import Testing

@Test func aBundleOpensFromItsFolderAndDraws() throws {
    let session = try ScaenaSession(directory: b1)
    let states = try session.states()
    #expect(states.count == 40)
    #expect(states.first == "cover")
    let timeline = try session.timeline()
    #expect(timeline.map(\.state) == states)
    #expect(timeline[0].slide == "cover")
    #expect(try !session.frame("cover").isEmpty)
    let pixels = try session.pixels("cover", width: 160)
    #expect(pixels.width == 160 && pixels.height == 90)
    #expect(pixels.rgba.count == 160 * 90 * 4)
}

@Test func anEditIsThePatchTheBrowserMakes() throws {
    let session = try ScaenaSession(directory: b1)
    let source = try session.source()
    let compiled = try session.compile(source)
    #expect(compiled.valid && compiled.error == nil)
    #expect(compiled.states.first?.state == "cover")
    let linted = try session.lint()
    #expect(linted.whole && linted.laid)
    let state = try session.states()[0]
    let inspected: JSONValue = try session.call("inspect", ["state": .string(state)])
    let node = try #require(inspected["looks"]?.object?.keys.sorted().first)
    let op: JSONValue = [
        "op": "replace_text", "state": .string(state), "node": .string(node), "from": 0, "to": 0, "text": "Set in Swift. ",
    ]
    let called = try session.tool("deck_patch", ["ops": [op]])
    #expect(called.edited)
    #expect(try session.source().contains("Set in Swift."))
}

@Test func aSaveIsWrittenZippedReopenedAndAdopted() throws {
    let session = try ScaenaSession(directory: b1)
    let saved = try session.save(subset: true)
    #expect(saved.files.contains("deck.json"))
    #expect(saved.file("deck.json") != nil)
    #expect(saved.file("no/such/file") == nil)
    let reopened = try ScaenaSession(zip: try saved.zip())
    #expect(try reopened.states() == session.states())
    try session.adopt(saved)
}

/// The Mac's save away from the window (lift-up PERF-5): a snapshot taken on the session's thread
/// is saved on another, the bytes the session's own save writes, and the session goes on from it;
/// an edit made while it is written stays, nothing adopted.
@Test func aSnapshotIsSavedAwayFromTheSessionAndAdoptedOnlyAsItWasTaken() async throws {
    let session = try ScaenaSession(directory: b1)
    let snapshot = try session.snapshot()
    let saved = try await Task.detached { try snapshot.save(subset: true) }.value
    #expect(saved.files.contains("deck.json"))
    #expect(Set(saved.files) == Set(try session.save(subset: true).files), "the files the session's save writes")
    #expect(try session.adopt(saved, since: snapshot), "unchanged since")

    let next = try session.snapshot()
    let state = try session.states()[0]
    let op: JSONValue = ["op": "set_state", "id": .string(state), "prop": "notes", "value": "Said while saving."]
    #expect(try session.tool("deck_patch", ["ops": [op]]).edited)
    let later = try await Task.detached { try next.save(subset: true) }.value
    #expect(try !session.adopt(later, since: next), "changed since: nothing adopted")
    #expect(try session.source().contains("Said while saving."), "the edit is kept")
}

@Test func whatTheEngineCannotDoIsAnErrorThatSaysWhy() throws {
    let session = try ScaenaSession(directory: b1)
    #expect(throws: ScaenaError.self) { try session.call("nonsense") as JSONValue }
    #expect(throws: ScaenaError.self) { try session.frame("no-such-state") }
    #expect(throws: ScaenaError.self) { try ScaenaSession(files: [:]) }
    #expect(throws: ScaenaError.self) { try ScaenaSession(zip: Data("not a zip".utf8)) }
}

/// A save on the Mac says what it did with a history it could not read, or one a newer Scaena
/// wrote, in the words the browser's status line says (lift-up 20261025b REL-1).
@Test func aSaveSaysWhatItDidWithAHistoryItCouldNotRead() throws {
    let session = try ScaenaSession(directory: b1)
    try session.keepHistory()
    let begun = try session.save(subset: false)
    var files: [String: Data] = [:]
    for path in begun.files { files[path] = begun.file(path) }
    let history = try #require(files["history/deck.loro"])
    #expect(DeckEditor.said(ofSave: begun.summary) == nil, "a history begun says nothing of it")

    // Cut short: kept aside, and the history begins again.
    var cut = files
    cut["history/deck.loro"] = history.prefix(history.count / 2)
    let damaged = try ScaenaSession(files: cut).save(subset: false)
    let aside = try #require(damaged.summary["again"]?.string)
    #expect(damaged.files.contains(aside))
    #expect(DeckEditor.said(ofSave: damaged.summary)
        == "Its history could not be read: it is kept aside as \(aside), and the history begins again")

    // A newer Scaena's: left as it is, the save unrecorded in it.
    var newer = files
    var future = history
    future.replaceSubrange(20..<22, with: [0xFF, 0xFF])
    newer["history/deck.loro"] = future
    let left = try ScaenaSession(files: newer).save(subset: false)
    #expect(left.file("history/deck.loro") == future, "the history left as it was")
    let said = try #require(DeckEditor.said(ofSave: left.summary))
    #expect(said.hasPrefix("This bundle's history was written by a newer Scaena"), "\(said)")
}
