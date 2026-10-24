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
