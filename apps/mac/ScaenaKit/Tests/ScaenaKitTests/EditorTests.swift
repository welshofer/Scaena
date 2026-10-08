import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// What the inspector, the layers, and the canvas read (PLAN 3.4): the browser's results, typed.
@Test func theInspectorAndTheCanvasReadWhatTheBrowserReads() throws {
    let session = try ScaenaSession(directory: b1)
    let choices = try session.choices(state: "cover", node: "title")
    #expect(choices.node == "title" && choices.type == "text")
    let role = try #require(choices.fields.first { $0.prop == "role" })
    #expect(role.value?.string == "display")
    #expect(role.lives == .node)
    if case .name(let of, let names, _) = role.takes {
        #expect(of == "text-role" && names.contains("headline"))
    } else {
        Issue.record("a role is one of the theme's names: \(role.takes)")
    }
    let state = try session.stateChoices(state: "cover")
    #expect(state.node == nil)
    #expect(state.fields.first { $0.prop == "layout" }?.lives == .state("cover"))

    let layers = try session.layers(state: "cover")
    #expect(Set(layers.map(\.node)) == ["title", "subtitle"])
    let boxes = try session.boxes(state: "cover")
    let title = try #require(boxes.first { $0.node == "title" })
    #expect(title.corners.count == 4 && title.locked == nil)
    let middle = CGPoint(x: title.rect[0] + title.rect[2] / 2, y: title.rect[1] + title.rect[3] / 2)
    let hits = try session.hits(state: "cover", at: middle)
    #expect(hits.first?.node == "title")

    // The cue of a state that has one, as the timeline places it.
    let cue = try session.cue(state: "goal")
    #expect(cue.span == 420 && cue.transition.duration > 0)
    #expect(try session.digest(state: "cover").count == 16)
    let pixels = try session.pixels("cover", width: 160)
    let image = try #require(pixels.image)
    #expect(image.width == 160 && image.height == 90)
}

/// The editor's loop (PLAN 2.3, 3.4): a choice is a patch, the deck's source compiled again and
/// the state shown linted; a finding's fix is one click; and each gives back the source an undo
/// makes the deck again.
@Test func aChoiceIsLintedAndItsFixIsOneClick() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    #expect(editor.valid && editor.error == nil)
    #expect(editor.whole && editor.findings.isEmpty)
    #expect(editor.slots.count == 40)
    editor.shown = "cover"
    let original = editor.source
    let revision = editor.revision

    // The title in the color of what lies behind it: it does not read.
    let choose: JSONValue = [
        "op": "choose", "node": "title", "prop": "style/color", "value": "surface", "state": "cover",
    ]
    #expect(try editor.make([choose]) == original)
    #expect(editor.revision > revision)
    #expect(editor.source != original)
    let faint = try #require(editor.findings.first { $0.node == "title" && $0.fixable })
    #expect(faint.code == "E110" || faint.code == "E111")
    #expect(faint.severity == .error && faint.at != nil)

    // Its fix, one click: the title reads.
    let chosen = editor.source
    #expect(try editor.fix(faint) == chosen)
    #expect(!editor.findings.contains { $0.node == "title" && $0.code == faint.code })

    // Undone, the deck is the source it had; redone, the fix again.
    let fixed = editor.source
    #expect(editor.restore(chosen) == fixed)
    #expect(editor.source == chosen)
    #expect(editor.restore(fixed) == chosen)
    #expect(editor.source == fixed)

    // A choice of what the node has already changes nothing, and says so.
    let same: JSONValue = ["op": "choose", "node": "title", "prop": "role", "value": "display", "state": "cover"]
    #expect(throws: ScaenaError.self) { try editor.make([same]) }

    // Every state linted once edits stop.
    editor.lintEvery()
    #expect(editor.whole)
}

/// A source typed is compiled as it stands (PLAN 2.3): one that does not compile says why, and
/// the deck stays what it was; one that does is the deck.
@Test func aSourceTypedIsCompiledAsItStands() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let source = editor.source
    #expect(editor.type(source + "\nstate {") == nil)
    #expect(editor.error != nil && !editor.valid)
    #expect(editor.findings.first?.code == editor.error?.code)
    // No gesture edits a source that does not compile.
    let choose: JSONValue = ["op": "choose", "node": "title", "prop": "role", "value": "headline", "state": "cover"]
    #expect(throws: ScaenaError.self) { try editor.make([choose]) }

    let title = try #require(source.range(of: "Scaena"))
    let typed = source.replacingCharacters(in: title, with: "Typed")
    #expect(editor.type(typed) == source)
    #expect(editor.valid && editor.error == nil)
    #expect(try editor.session.source().contains("Typed"))
    // What the pane already says compiles nothing.
    #expect(editor.type(typed) == nil)
}

/// A state's drawing is painted again only when it changed (PLAN 2.35).
@Test func aStatesDrawingIsKeptUntilItChanges() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let first = try #require(editor.drawing("cover", width: 160))
    let again = try #require(editor.drawing("cover", width: 160))
    #expect(first === again)
    let choose: JSONValue = ["op": "choose", "node": "title", "prop": "role", "value": "headline", "state": "cover"]
    try editor.make([choose])
    let changed = try #require(editor.drawing("cover", width: 160))
    #expect(changed !== first)
}
