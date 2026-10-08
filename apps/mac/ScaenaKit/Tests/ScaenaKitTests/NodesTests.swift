import AppKit
import Foundation
import ScaenaKit
import Testing

/// Lock's patch (PLAN 2.95, 3.11), as the browser's: each node's own `locked`, its id escaped as a
/// JSON pointer; unlocked where every one is locked by its own lock, locked otherwise.
@Test func lockingIsEachNodesOwnLock() {
    let none = locking(["a/b", "c~d"], own: { _ in false })
    #expect(none.locking)
    #expect(none.ops == [
        ["op": "add", "path": "/nodes/a~1b/locked", "value": true],
        ["op": "add", "path": "/nodes/c~0d/locked", "value": true],
    ])
    let all = locking(["a", "b"], own: { _ in true })
    #expect(!all.locking && all.ops == [["op": "remove", "path": "/nodes/a/locked"], ["op": "remove", "path": "/nodes/b/locked"]])
    // Some locked: the rest are locked too.
    let some = locking(["a", "b"], own: { $0 == "a" })
    #expect(some.locking && some.ops == [["op": "add", "path": "/nodes/b/locked", "value": true]])
    #expect(locking(["a"], own: { _ in true }, on: true).ops.isEmpty, "locked already")
}

/// Insert, ⌘D, Delete, and Lock (PLAN 3.11), as the browser's (PLAN 2.34, 2.79, 2.95): what the
/// deck offers, inserted about a point and entering in the state shown; a copy beside it; a
/// node taken out of the deck where no state shows it after; and a node locked by its own lock.
@Test func aNodeIsInsertedCopiedDeletedAndLocked() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let offered = try session.inserts()
    let n = try #require(offered.firstIndex { $0.label == "Shape · rect" })
    #expect(offered[n].kind == "Shape" && offered[n].name == "rect")
    #expect(offered[n].node["type"]?.string == "shape")

    let added = try session.inserting(state: "cover", n: n, at: CGPoint(x: 1500, y: 900))
    #expect(added.cell.count == 4 && added.patch.first?["op"]?.string == "add_node")
    try editor.make(added.patch)
    #expect(try session.boxes(state: "cover").contains { $0.node == added.id })

    let copy = try session.duplicating(state: "cover", node: added.id)
    #expect(copy.id != added.id)
    try editor.make(copy.patch)
    #expect(try session.boxes(state: "cover").contains { $0.node == copy.id })

    // Lock: the canvas passes over it, in every state; unlocked again.
    let lock = locking([copy.id], own: { _ in false })
    try editor.make(lock.ops)
    #expect(try session.boxes(state: "cover").first(where: { $0.node == copy.id })?.locked == copy.id)
    try editor.make(locking([copy.id], own: { _ in true }).ops)
    #expect(try session.boxes(state: "cover").first(where: { $0.node == copy.id })?.locked == nil)

    // Delete: shown in no state after the cover, the copy goes from the deck.
    let gone = try session.deleting(state: "cover", node: copy.id)
    #expect(gone.allSatisfy { $0["op"]?.string == "remove_node" })
    try editor.make(gone)
    #expect(try !session.boxes(state: "cover").contains { $0.node == copy.id })
    #expect(throws: ScaenaError.self) { try session.deleting(state: "cover", node: "nobody") }
}

/// The canvas's keys (PLAN 3.11): with no text typed in, the commands they make are the canvas's;
/// while one is, the text's.
@MainActor
@Test func theCanvasKeysAreTheTextsWhileOneIsTypedIn() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let typing = Typing(editor: editor)
    let keys = CanvasKeys(typing: typing)
    var commands: [Selector] = []
    keys.command = { selector in
        commands.append(selector)
        return true
    }
    #expect(keys.acceptsFirstResponder)
    keys.doCommand(by: #selector(NSStandardKeyBindingResponding.deleteBackward(_:)))
    keys.doCommand(by: #selector(NSStandardKeyBindingResponding.cancelOperation(_:)))
    #expect(commands == [
        #selector(NSStandardKeyBindingResponding.deleteBackward(_:)),
        #selector(NSStandardKeyBindingResponding.cancelOperation(_:)),
    ])
    // Characters typed with no text typed in change nothing.
    let source = editor.source
    keys.insertText("x", replacementRange: NSRange(location: NSNotFound, length: 0))
    #expect(editor.source == source)

    #expect(typing.enter("title", in: "cover", at: nil))
    keys.doCommand(by: #selector(NSStandardKeyBindingResponding.deleteBackward(_:)))
    #expect(commands.count == 2, "the text's, not the canvas's")
    #expect(typing.carets?.text == "Scaen")
}
