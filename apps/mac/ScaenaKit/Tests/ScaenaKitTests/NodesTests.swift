#if os(macOS)
import AppKit
#endif
import CoreGraphics
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

#if os(macOS)  // the canvas's keys are AppKit's; the iPad's come with PLAN 4.4 and 4.6
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
#endif

/// Words in a shape (PLAN 2.108), as the browser gives them: a star takes words; the patch that
/// gives it some adds an empty text inside it, one step to undo, which its box then names and the
/// pointer over which shows the star; and a shape that holds words gives them back with nothing to
/// add.
@Test func aShapeIsGivenWordsThatStandInsideIt() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let n = try #require(try session.inserts().firstIndex { $0.label == "Shape · star" })
    let star = try session.inserting(state: "cover", n: n, at: CGPoint(x: 1500, y: 700))
    try editor.make(star.patch)
    func shape() throws -> NodeBox { try #require(try session.boxes(state: "cover").first { $0.node == star.id }) }
    let bare = try shape()
    #expect(bare.takesWords == true && bare.words == nil)
    let given = try session.wording(state: "cover", shape: star.id)
    #expect(given.id == "\(star.id)-words" && given.patch.count == 1)
    let before = try editor.make(given.patch)
    #expect(try shape().words == given.id)
    let outer = try shape().rect
    let inner = try #require(try session.boxes(state: "cover").first { $0.node == given.id }).rect
    #expect(inner[0] > outer[0] && inner[1] > outer[1], "inside the star")
    #expect(inner[0] + inner[2] < outer[0] + outer[2] && inner[1] + inner[3] < outer[1] + outer[3], "inside the star")
    let middle = CGPoint(x: inner[0] + inner[2] / 2, y: inner[1] + inner[3] / 2)
    #expect(try session.boxes(state: "cover").under(middle) == star.id, "the pointer over the words shows the star")
    #expect(try session.boxes(state: "cover").reached(given.id) == star.id, "a press on the words reaches the star")
    #expect(try session.boxes(state: "cover").reached(star.id) == star.id)
    #expect(try session.wording(state: "cover", shape: star.id).patch.isEmpty, "given back, nothing added")
    editor.restore(before)
    #expect(try shape().words == nil, "one step to undo")
}

/// Connectors (PLAN 2.109), as the browser's canvas makes them: B1's title joined to its subtitle
/// by an arrow, one step to undo, whose box names what it joins and whose line the pointer picks
/// it by; a point of its box off the line is not it.
@Test func twoNodesAreJoinedByAnArrowPickedByItsLine() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let joined = try session.connecting(state: "cover", from: "title", to: "subtitle", arrow: true)
    #expect(joined.id == "title-to-subtitle")
    let before = try editor.make(joined.patch)
    let arrow = try #require(try session.boxes(state: "cover").first { $0.node == joined.id })
    #expect(arrow.joins == ["title", "subtitle"])
    let route = try #require(arrow.route)
    #expect(route.count == 2)
    let middle = CGPoint(x: (route[0][0] + route[1][0]) / 2, y: (route[0][1] + route[1][1]) / 2)
    #expect(arrow.holds(middle), "on its line")
    #expect(!arrow.holds(CGPoint(x: middle.x + 40, y: middle.y)), "off its line")
    editor.restore(before)
    #expect(try session.boxes(state: "cover").first { $0.node == joined.id } == nil, "one step to undo")
}

/// Connectors go where what they join goes (PLAN 2.109), as the browser's canvas takes them: B1's
/// title, its subtitle, and the arrow joining them duplicated together, the copy of the arrow
/// joining the copies; then the copies deleted together, the copy of the arrow with them; each one
/// patch.
@Test func aConnectorIsDuplicatedAndDeletedWithWhatItJoins() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let joined = try session.connecting(state: "cover", from: "title", to: "subtitle", arrow: true)
    try editor.make(joined.patch)
    let made = try session.duplicating(state: "cover", nodes: ["title", "subtitle", joined.id])
    #expect(made.ids.count == 3)
    try editor.make(made.patch)
    let copy = try #require(try session.boxes(state: "cover").first { $0.node == made.ids[2] })
    #expect(copy.joins == [made.ids[0], made.ids[1]], "the copy of the arrow joins the copies")
    let gone = try session.deleting(state: "cover", nodes: made.ids)
    try editor.make(gone)
    #expect(try !session.boxes(state: "cover").contains { made.ids.contains($0.node) })
}

/// Diagrams (PLAN 2.110), as the browser's canvas draws them: B1's subtitle, given three lines,
/// drawn as a process in its place, a box for each line with its words, an arrow between each two,
/// the subtitle gone from the cover; one step to undo; and a hierarchy refused for lines at one
/// level.
@Test func aTextsLinesAreDrawnAsADiagramInItsPlace() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let lines: JSONValue = .object([
        "op": .string("replace"), "path": .string("/nodes/subtitle/text"), "value": .string("Why\nWhat\nRefuse"),
    ])
    _ = try editor.make([lines])
    let made = try session.diagramming(state: "cover", node: "subtitle", kind: .process)
    #expect(made.id == "subtitle-process")
    let before = try editor.make(made.patch)
    let boxes = try session.boxes(state: "cover")
    let held = boxes.filter { $0.parent == made.id }.map(\.node)
    #expect(held.count == 3, "a box for each line: \(held)")
    let arrows = boxes.filter { box in box.joins.map { ends in ends.allSatisfy { held.contains($0) } } ?? false }
    #expect(arrows.count == 2, "an arrow between each two")
    #expect(boxes.first { $0.node == "subtitle" } == nil, "the text gone from the cover")
    editor.restore(before)
    #expect(try session.boxes(state: "cover").first { $0.node == made.id } == nil, "one step to undo")
    #expect(throws: ScaenaError.self) { try session.diagramming(state: "cover", node: "subtitle", kind: .hierarchy) }
}

/// The pen (PLAN 2.114), as the browser's canvas draws with it: a stroke across B1's cover is a new
/// drawing where it was drawn; a second stroke joins it, its box grown to take both in, what it
/// drew first where it was; each one step to undo.
@Test func aStrokeOfThePenIsADrawingThatLaterStrokesJoin() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let wave = (0...40).map { i in CGPoint(x: 200 + Double(i) * 10, y: 700 + 40 * sin(Double(i) / 4)) }
    let first = try session.inking(state: "cover", points: wave, onto: nil)
    #expect(first.id == "drawing")
    let blank = try editor.make(first.patch)
    let drawn = try #require(try session.boxes(state: "cover").first { $0.node == first.id })
    #expect(drawn.rect[0] <= 200 && drawn.rect[0] + drawn.rect[2] >= 600, "where it was drawn: \(drawn.rect)")
    let line = (0...10).map { i in CGPoint(x: 300 + Double(i) * 20, y: 900) }
    let second = try session.inking(state: "cover", points: line, onto: first.id)
    #expect(second.id == first.id, "the second stroke joins the drawing")
    let one = try editor.make(second.patch)
    let grown = try #require(try session.boxes(state: "cover").first { $0.node == first.id })
    #expect(grown.rect[0] == drawn.rect[0] && grown.rect[1] == drawn.rect[1], "what it drew first stays: \(grown.rect)")
    #expect(grown.rect[1] + grown.rect[3] >= 900, "grown to take the second in: \(grown.rect)")
    editor.restore(one)
    #expect(try session.boxes(state: "cover").first { $0.node == first.id }?.rect == drawn.rect, "one step to undo")
    editor.restore(blank)
    #expect(try session.boxes(state: "cover").first { $0.node == first.id } == nil, "and another the drawing")
}
