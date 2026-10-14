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

/// An edit undone after the window saved the deck, as it saves one by itself (PLAN 3.3): the save
/// names B1's fonts by their content and the session goes on from it, while the source the edit
/// replaced names them as they were. It still opens: the undo makes it the deck again, and the
/// redo the edit.
@Test func anEditIsUndoneAfterTheDeckIsSaved() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let original = editor.source
    let typed: JSONValue = [
        "op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Saved. ",
    ]
    #expect(try editor.make([typed]) == original)
    let edited = editor.source
    try editor.session.adopt(try editor.session.save(subset: true))
    #expect(try !editor.session.source().contains("-VF.ttf"), "the save names the fonts by their content")

    let revision = editor.revision
    #expect(editor.restore(original) == edited, "the source from before the save opens")
    #expect(editor.valid && editor.revision > revision)
    #expect(try !editor.session.source().contains("Saved. "))
    #expect(editor.restore(edited) == original)
    #expect(try editor.session.source().contains("Saved. "))
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

/// PageDown and PageUp on the canvas (PLAN 4.12, as the browser's, PLAN 2.124): the next state on
/// the deck's timeline, a slide or a step of one, and the one before; none past either end, or
/// from a state the deck lacks. B1's cover is its first slide, and its second, goal, has two steps.
@Test func pageDownAndPageUpGoAlongTheDecksStates() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let states = editor.slots.map(\.state)
    #expect(Array(states.prefix(4)) == ["cover", "goal", "goal-why", "goal-bar"])
    #expect(editor.state(from: "cover", by: 1) == "goal", "the next slide")
    #expect(editor.state(from: "goal", by: 1) == "goal-why", "a step of it")
    #expect(editor.state(from: "goal-bar", by: 1) == "belief-1", "the slide after its steps")
    #expect(editor.state(from: "goal-why", by: -1) == "goal", "the one before")
    #expect(editor.state(from: "cover", by: -1) == nil, "none before the first")
    let last = try #require(states.last)
    #expect(editor.state(from: last, by: 1) == nil, "none past the last")
    #expect(editor.state(from: last, by: -1) == states[states.count - 2])
    #expect(editor.state(from: "nowhere", by: 1) == nil, "none from a state the deck lacks")
}

/// B1's files by their paths in it, `deck.json` changed by `edit`.
private func b1Files(_ edit: (String) throws -> String) throws -> [String: Data] {
    let root = b1.resolvingSymlinksInPath()
    var files: [String: Data] = [:]
    for path in try FileManager.default.subpathsOfDirectory(atPath: root.path) {
        guard !path.split(separator: "/").contains(where: { $0.hasPrefix(".") }) else { continue }
        var folder: ObjCBool = false
        let url = root.appending(path: path)
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &folder), !folder.boolValue else { continue }
        files[path] = try Data(contentsOf: url)
    }
    let deck = String(decoding: try #require(files["deck.json"]), as: UTF8.self)
    files["deck.json"] = Data(try edit(deck).utf8)
    return files
}

/// B1 as a build that writes deck format `version` saved it.
private func savedB1(in version: String) throws -> [String: Data] {
    try b1Files { deck in
        let saved = try #require(deck.range(of: #""scaena": "[0-9.]+""#, options: .regularExpression))
        return deck.replacingCharacters(in: saved, with: #""scaena": "\#(version)""#)
    }
}

/// A deck an older build saved (SPEC §3.1), as one downloaded from a site built before the app
/// was: it opens in the current format with every state shown, where it once showed no slide.
@Test func aDeckSavedByAnOlderBuildOpensWithEveryState() throws {
    let editor = DeckEditor(session: try ScaenaSession(files: try savedB1(in: "0.15")))
    #expect(editor.valid && editor.error == nil)
    #expect(editor.slots.count == 40)
    #expect(editor.unshown == nil)
    #expect(editor.source.split(separator: "\n").first?.contains("scaena:") == false)
}

/// A deck a newer build saved is not opened: the alert says why, in words.
@Test func aDeckSavedByANewerBuildIsRefusedSayingWhy() throws {
    let files = try savedB1(in: "0.99")
    let refused = #expect(throws: ScaenaError.self) { _ = try ScaenaSession(files: files) }
    #expect(refused?.localizedDescription.hasPrefix("deck.json: deck format 0.99 is newer than this build reads") == true)
}

/// A deck that opens but is no deck yet, a role its theme lacks: the window says why in place
/// of a slide, and shows no state.
@Test func aDeckThatDoesNotValidateSaysWhyItShowsNothing() throws {
    let files = try b1Files { $0.replacingOccurrences(of: #""role": "display""#, with: #""role": "nope""#) }
    let editor = DeckEditor(session: try ScaenaSession(files: files))
    #expect(!editor.valid && editor.slots.isEmpty)
    let why = try #require(editor.unshown)
    #expect(why.contains("text role `nope` is not in the theme"))
}

/// Each step of undo is named by what it does (PLAN 3.28, 4.11), as the Edit menu says it after
/// Undo and Redo and the iPad's Undo reads it: what the engine's own patches insert and delete, a
/// move, a property chosen as the inspector calls it, a turn, words typed, a slide moved, and the
/// slide's own property; a patch of nothing it knows is a change, never a step with no name.
@Test func aStepOfUndoIsNamedByWhatItDoes() throws {
    let session = try ScaenaSession(directory: b1)
    let label = { (prop: String) in prop == "style/size" ? "Size" : prop }
    let copied = try session.duplicating(state: "cover", node: "title").patch
    let hidden = try session.deleting(state: "cover", node: "title")
    let removed = try session.deleting(state: "cover", node: "title", everywhere: true)
    #expect(StepName.of(copied, label: label) == "Insert")
    #expect(StepName.of(hidden, label: label) == "Delete" && StepName.of(removed, label: label) == "Delete")
    #expect(StepName.of([["op": "place", "node": "title", "state": "cover", "at": ["in": "grid"]]], label: label) == "Move")
    #expect(StepName.of([["op": "choose", "node": "title", "prop": "style/size", "value": "xl"]], label: label) == "Change Size")
    #expect(StepName.of([["op": "choose", "node": "title", "prop": "transform/rotate", "value": 15]], label: label) == "Rotate")
    #expect(StepName.of([["op": "replace_text", "node": "title", "from": 0, "to": 0, "text": "A"]], label: label) == "Typing")
    #expect(StepName.of([["op": "move_slide", "slide": "goal", "after": "cover"]], label: label) == "Move Slide")
    #expect(StepName.of([["op": "set_state", "id": "cover", "prop": "layout", "value": "title"]], label: label) == "Change layout")
    #expect(StepName.of([["op": "add", "path": "/nodes/title/locked", "value": true]], label: label) == "Change")
}
