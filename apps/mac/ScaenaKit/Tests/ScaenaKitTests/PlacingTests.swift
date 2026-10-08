import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// Targets as `targets` gives them, held `by` a container of that kind.
private func held(by: String) throws -> Targets {
    let json = #"{"by": "\#(by)", "cell": [0, 0, 100, 50], "within": [0, 0, 1920, 1080], "snaps": []}"#
    return try JSONDecoder().decode(Targets.self, from: Data(json.utf8))
}

/// How a drag snaps, as the browser's canvas decides it (`snapOf`): by the node's placement and
/// what holds it, a handle resizing, Shift taking a node off the theme's grid or back onto it.
@Test func aDragSnapsAsTheBrowsersCanvasDecides() throws {
    let grid = try held(by: "grid")
    let inSlot: JSONValue = ["in": "title"]
    let onCells: JSONValue = ["col": [1, 4], "row": [1, 2]]
    let offGrid: JSONValue = ["rect": [10, 10, 100, 50]]
    #expect(grid.snap(at: inSlot, resize: false, shift: false) == .slot)
    #expect(grid.snap(at: inSlot, resize: true, shift: false) == nil)
    #expect(grid.snap(at: inSlot, resize: false, shift: true) == .free)
    #expect(grid.snap(at: onCells, resize: false, shift: false) == .move)
    #expect(grid.snap(at: onCells, resize: true, shift: false) == .resize)
    #expect(grid.snap(at: offGrid, resize: false, shift: false) == .free)
    #expect(grid.snap(at: offGrid, resize: false, shift: true) == .move, "Shift puts it back on the grid")
    #expect(try held(by: "stack").snap(at: nil, resize: false, shift: false) == .order)
    #expect(try held(by: "stack").snap(at: nil, resize: true, shift: false) == nil)
    #expect(try held(by: "frame").snap(at: nil, resize: true, shift: false) == .free)
    #expect(try held(by: "cells").snap(at: ["area": "left"], resize: false, shift: false) == .slot)
    #expect(offGrid.offGrid && !inSlot.offGrid)
    let framed: JSONValue = ["rect": [0, 0, 10, 10], "parent": "panel"]
    #expect(!framed.offGrid, "a frame places its children by rect")
}

/// A drag on B1's cover, as the window makes one (PLAN 3.7): the title drawn moved as it goes,
/// laying nothing out; into another slot, or off the grid kept to the state with Option; the drop
/// one patch, which the window's undo takes; and the title off the grid flagged, as lint flags it.
@Test func aDragMovesTheTitleAsTheBrowsersCanvasDoes() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let targets = try session.targets(state: "cover", node: "title")
    #expect(targets.by == "grid")
    let subtitle = try #require(targets.slots["subtitle"])

    let rest = try session.digest(state: "cover")
    let drawn = editor.drawn
    editor.move(["title"], by: CGVector(dx: 40, dy: 30))
    #expect(editor.drawn > drawn, "the canvas is drawn again")
    #expect(try session.digest(state: "cover") != rest, "the title is drawn moved")
    editor.still()
    #expect(try session.digest(state: "cover") == rest)

    let slotted = try #require(try session.snap(state: "cover", node: "title", how: .slot, to: subtitle))
    #expect(slotted.patch.first?["at"]?["in"]?.string == "subtitle")

    let left = targets.cell.offsetBy(dx: 40, dy: 30)
    let freed = try #require(try session.snap(state: "cover", node: "title", how: .free, to: left, fork: true))
    #expect(freed.patch.first?["at"]?["rect"] != nil)
    #expect(try session.reach(freed.patch).contains("cover"))

    let before = editor.source
    let undo = try editor.make(freed.patch)
    #expect(undo == before, "the source it replaced, for the window's undo")
    #expect(try session.placements(state: "cover")["title"]?.offGrid == true)
    #expect(editor.findings.contains { $0.code == "W301" && $0.node == "title" }, "lint flags it off the grid")
    #expect(editor.restore(undo) != nil)
    #expect(try session.placements(state: "cover")["title"]?.offGrid == false)
}
