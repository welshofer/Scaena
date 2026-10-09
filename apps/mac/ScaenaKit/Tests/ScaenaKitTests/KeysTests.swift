#if os(macOS)
import AppKit
#else
import UIKit
#endif
import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// The torture deck (PLAN 0.2): containers, a stack, shapes, and images.
private let torture = repository.appending(path: "tests/fixtures/torture.scaena")

/// `node`'s box among `boxes`, canvas units.
private func box(_ boxes: [NodeBox], _ node: String) -> CGRect? {
    boxes.first { $0.node == node }.map { CGRect(x: $0.rect[0], y: $0.rect[1], width: $0.rect[2], height: $0.rect[3]) }
}

/// How a state reads to VoiceOver (PLAN 3.17): each node a reader hears, in turn, a heading at its
/// level, a figure by its alt text; a state the deck lacks refused.
@Test func aStateReadsNodeByNodeAsVoiceOverHearsIt() throws {
    let session = try ScaenaSession(directory: b1)
    let reads = try session.reads(state: "cover")
    #expect(reads.map(\.node) == ["title", "subtitle"])
    #expect(reads[0].role == "heading" && reads[0].level == 1 && reads[0].text == "Scaena")
    #expect(reads[1].role == "heading" && reads[1].level == 2)
    #expect(throws: ScaenaError.self) { try session.reads(state: "nowhere") }
    let images = try ScaenaSession(directory: torture).reads(state: "images")
    #expect(images.contains { $0.node == "image-focal" && $0.role == "figure" && $0.text.hasPrefix("The test card, cover") })
}

/// The canvas by keys (PLAN 3.17, as the browser's, PLAN 2.75): Tab's reading order, a container
/// where the first it holds is painted; an arrow steps a node a track on the grid, or its end a
/// track with Shift, and past the next child in a stack, each the patch its drag makes.
@Test func theKeysStepThroughAndMoveNodesAsTheBrowsersDo() throws {
    let session = try ScaenaSession(directory: torture)
    let boxes = try session.boxes(state: "containers")
    #expect(readingOrder(boxes, in: nil) == ["case", "stats", "tally", "board", "card", "marks"])
    #expect(readingOrder(boxes, in: "stats") == ["stat-a", "stat-b", "stat-c"])
    #expect(readingOrder(boxes, in: "stat-a") == ["stat-a-figure", "stat-a-label"])

    let panel = try session.targets(state: "shapes", node: "shape-panel")
    #expect(panel.columns.count == 12 && panel.columns[1].lowerBound == 242 && panel.columns[1].upperBound == 364)
    let placed = try session.placements(state: "shapes")["shape-panel"]
    let how = try #require(panel.snap(at: placed, resize: false, shift: false))
    #expect(how == .move)
    let right = try #require(panel.stepped("shape-panel", dx: 1, dy: 0, how: how, grow: false) { _ in nil })
    #expect(right == CGRect(x: 242, y: 210, width: 560, height: 318), "to the next column's start")
    let left = panel.stepped("shape-panel", dx: -1, dy: 0, how: how, grow: false) { _ in nil }
    #expect(left == nil, "at the edge")
    let wider = try #require(panel.stepped("shape-panel", dx: 1, dy: 0, how: .resize, grow: true) { _ in nil })
    #expect(wider == CGRect(x: 96, y: 210, width: 706, height: 318), "its end to the next column's end")
    let snapped = try #require(try session.snap(state: "shapes", node: "shape-panel", how: how, to: right))
    #expect(snapped.patch.first?["at"]?["col"] == [2, 5])
    #expect(try session.tool("deck_patch", ["ops": .array(snapped.patch)]).edited)

    let stat = try session.targets(state: "containers", node: "stat-a")
    #expect(stat.flow == ["stat-a", "stat-b", "stat-c"])
    let past = try #require(stat.stepped("stat-a", dx: 1, dy: 0, how: .order, grow: false) { box(boxes, $0) })
    let order = try #require(try session.snap(state: "containers", node: "stat-a", how: .order, to: past))
    #expect(try session.tool("deck_patch", ["ops": .array(order.patch)]).edited)
    #expect(try session.targets(state: "containers", node: "stat-a").flow == ["stat-b", "stat-a", "stat-c"])
}

/// A selection built by keys (PLAN 3.17, as the browser's, PLAN 2.89): Space puts the node the keys
/// are on in what is selected, after the rest, or takes it out, the next then the first; one
/// another container holds starts it anew.
@Test func spaceBuildsASelectionAsTheBrowsersDoes() throws {
    let boxes = try ScaenaSession(directory: torture).boxes(state: "containers")
    let holder = { (node: String) in boxes.first { $0.node == node }?.parent }
    var selected = toggled("stat-a", in: [], holder: holder)
    #expect(selected == ["stat-a"])
    selected = toggled("stat-b", in: selected, holder: holder)
    selected = toggled("stat-c", in: selected, holder: holder)
    #expect(selected == ["stat-a", "stat-b", "stat-c"])
    selected = toggled("stat-a", in: selected, holder: holder)
    #expect(selected == ["stat-b", "stat-c"], "taken out, the next is the first")
    #expect(toggled("case", in: selected, holder: holder) == ["case"], "another container's starts it anew")
    #expect(toggled("stat-b", in: ["stat-b"], holder: holder).isEmpty)
}

/// The handles by keys (PLAN 3.17, as the browser's, PLAN 2.75): in Tab's order, each arrow the
/// patch its drag makes, a hundredth or with Shift a tenth, a corner a radius step, and a crop side
/// across itself alone.
@Test func theHandlesMoveByKeysAsTheirDragsDo() throws {
    let session = try ScaenaSession(directory: torture)
    let tri = try session.outline(state: "shapes", node: "shape-tri")
    let panel = try session.outline(state: "shapes", node: "shape-panel")
    let image = try session.framing(state: "images", node: "image-focal")
    #expect(KeyedHandle.of(outline: tri, framing: nil) == [.point(0), .point(1), .point(2)])
    #expect(KeyedHandle.of(outline: panel, framing: nil) == [.corner])
    #expect(
        KeyedHandle.of(outline: nil, framing: image) == [.crop(.top), .crop(.right), .crop(.bottom), .crop(.left), .focal])

    let moved = try #require(
        KeyedHandle.point(0).nudged(dx: 1, dy: 0, far: false, outline: tri, framing: nil, state: "shapes", fork: false))
    #expect(moved.first?["value"] == [[0.51, 0], [1, 1], [0, 1]])
    #expect(try session.tool("deck_patch", ["ops": .array(moved)]).edited)
    let rounder = KeyedHandle.corner.nudged(dx: 1, dy: 0, far: false, outline: panel, framing: nil, state: "shapes", fork: false)
    #expect(rounder?.first?["value"] == "radius.4")
    let squarer = KeyedHandle.corner.nudged(dx: -1, dy: 0, far: false, outline: panel, framing: nil, state: "shapes", fork: false)
    #expect(squarer?.first?["value"] == "radius.2")
    let down = KeyedHandle.crop(.left).nudged(dx: 0, dy: 1, far: false, outline: nil, framing: image, state: "images", fork: false)
    #expect(down == nil, "a side moves across itself alone")
    let cropped = KeyedHandle.crop(.left).nudged(dx: 1, dy: 0, far: false, outline: nil, framing: image, state: "images", fork: false)
    #expect(cropped?.first?["value"] == [0.01, 0, 0.99, 1])
    let focal = try #require(
        KeyedHandle.focal.nudged(dx: 1, dy: 0, far: true, outline: nil, framing: image, state: "images", fork: true))
    #expect(focal.first?["value"] == [0.1, 0.5] && focal.first?["fork"] == true)
}

#if os(macOS)  // the canvas's keys are AppKit's; the iPad's come with PLAN 4.4 and 4.6
/// The canvas reads a key before the input system while no text is typed in (PLAN 3.17): what it
/// takes goes no further.
@MainActor
@Test func theCanvasReadsAKeyBeforeTheInputSystem() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let keys = CanvasKeys(typing: Typing(editor: editor))
    var heard: [UInt16] = []
    keys.pressed = { event in
        heard.append(event.keyCode)
        return true
    }
    for (characters, code) in [("\u{F703}", UInt16(124)), ("\t", UInt16(48)), (" ", UInt16(49))] {
        let event = try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
                characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code))
        keys.keyDown(with: event)
    }
    #expect(heard == [124, 48, 49])
}
#else
/// Escape and Return come to the iPad's canvas as key commands of its own, ahead of the system's,
/// since neither reaches it as a press (PLAN 4.6): none is offered until the canvas takes them, and
/// each goes to it, Return with Option too.
@MainActor
@Test func escapeAndReturnAreTheIPadCanvassOwnKeyCommands() throws {
    let presses = CanvasPresses()
    #expect(presses.keyCommands == nil)
    var heard: [String] = []
    presses.commanded = { input, flags in
        heard.append(flags.contains(.alternate) ? "⌥" + input : input)
        return true
    }
    let commands = try #require(presses.keyCommands)
    #expect(commands.allSatisfy(\.wantsPriorityOverSystemBehavior))
    let keyed = NSSelectorFromString("keyed:")
    let keys: [(String, UIKeyModifierFlags)] = [(UIKeyCommand.inputEscape, []), ("\r", []), ("\r", .alternate)]
    for (input, flags) in keys {
        let command = try #require(commands.first { $0.input == input && $0.modifierFlags == flags })
        #expect(presses.canPerformAction(keyed, withSender: command))
        _ = presses.perform(keyed, with: command)
    }
    #expect(heard == [UIKeyCommand.inputEscape, "\r", "⌥\r"])
}
#endif
