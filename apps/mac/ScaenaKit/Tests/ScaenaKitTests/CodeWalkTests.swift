import Foundation
import ScaenaKit
import Testing

/// Walk Through Lines… from the Mac and the iPad (PLAN 2.140, lift-up 20261124 FUNC-1), through
/// the session as the browser's editor makes it: one patch, the slide's state the first step and
/// a step after it for each set of lines after the first, the slides after them as they were, and
/// the deck as it was once the source before it is restored, as an undo restores it. The lines in
/// focus are set by their gutter's numbers, and nothing said shows every line again.
@Test func aWalkThroughAddsAStepForEachRange() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    _ = try editor.make(try editor.session.coding(node: "title", language: "rust"))
    let text = try #require(try editor.session.carets(state: "cover", node: "title")).text
    let end = JSONValue.number(Double(text.unicodeScalars.count))
    let lines: JSONValue = [
        "op": "replace_text", "node": "title", "state": "cover", "from": end, "to": end, "text": "\nfn a() {}\nfn b() {}",
    ]
    _ = try editor.make([lines])
    let code = try #require(try editor.session.carets(state: "cover", node: "title")?.code)
    #expect(code.from == 1)

    let before = try editor.session.states()
    let source = try editor.session.source()
    let walk = try editor.session.walking(state: "cover", node: "title", steps: ["1–3", "2", "3"])
    #expect(walk.filter { $0["op"] == "add_state" }.count == 2, "a step for each set of lines after the first")
    _ = try editor.make(walk)
    let after = try editor.session.states()
    #expect(after.count == before.count + 2)
    #expect(Array(after.suffix(before.count - 1)) == Array(before.dropFirst()), "the slides after it as they were")
    _ = editor.restore(source)
    #expect(try editor.session.states() == before, "undone, the deck as it was")

    let focused = try editor.session.focusing(state: "cover", node: "title", said: "2–3")
    #expect(!focused.isEmpty)
    _ = try editor.make(focused)
    #expect(try editor.session.focusing(state: "cover", node: "title", said: "2–3").isEmpty, "in focus already")
    #expect(!(try editor.session.focusing(state: "cover", node: "title", said: "")).isEmpty, "Clear Focus shows every line")
    #expect(throws: (any Error).self, "a step past the last line is refused") {
        try editor.session.walking(state: "cover", node: "title", steps: ["1–3", "9"])
    }
}
