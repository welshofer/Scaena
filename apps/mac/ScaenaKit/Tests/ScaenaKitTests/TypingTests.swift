import AppKit
import Foundation
import ScaenaKit
import Testing

/// Two lines as the engine sets them: "ab cd", ended by a line break, then "ef".
private let twoLines = #"""
    {"text": "ab cd\nef", "lines": [
      {"top": 0, "bottom": 10, "x": 0, "start": 0, "end": 6, "broken": true,
       "chars": [[0, 0, 5], [1, 5, 10], [2, 10, 13], [3, 13, 18], [4, 18, 23], [5, 23, 23]]},
      {"top": 10, "bottom": 20, "x": 0, "start": 6, "end": 8, "broken": false, "chars": [[6, 0, 5], [7, 5, 10]]}
    ]}
    """#

private func carets(_ json: String) throws -> Carets {
    try JSONDecoder().decode(Carets.self, from: Data(json.utf8))
}

/// Where a caret stands, read from the engine's carets as the browser reads them (PLAN 2.32):
/// nothing laid out on the Mac.
@Test func aCaretStandsWhereTheEngineSetTheCharacters() throws {
    let c = try carets(twoLines)
    #expect(c.length == 8 && c.items == nil)
    // A caret after the line break stands on the next line; before it, on its own.
    #expect(c.line(of: 6) == 1 && c.line(of: 5) == 0)
    let after = c.caret(at: 8)
    #expect(after.line == 1 && after.x == 10)
    let before = c.caret(at: 5)
    #expect(before.line == 0 && before.x == 23)
    #expect(c.offset(onLine: 1, nearest: 9) == 8)
    #expect(c.caret(near: CGPoint(x: 3, y: 15)).offset == 7)
    #expect(c.end(ofLine: 0) == 5 && c.end(ofLine: 1) == 8)
    #expect(c.covered(from: 1, to: 4) == [CGRect(x: 5, y: 0, width: 13, height: 10)])
    #expect(c.box(at: 3) == CGRect(x: 13, y: 0, width: 0, height: 10))

    // The text's own boundaries: characters as read, words, and paragraphs.
    #expect(c.before(8) == 7 && c.after(0) == 1)
    #expect(c.wordBefore(5) == 3 && c.wordAfter(3) == 5)
    #expect(c.word(at: 4) == NSRange(location: 3, length: 2))
    #expect(c.paragraph(at: 7) == NSRange(location: 6, length: 2))
    #expect(c.paragraph(at: 2) == NSRange(location: 0, length: 5))

    // Offsets are UTF-16, as the input system counts; `replace_text` counts characters.
    let wide = try carets(#"{"text": "aé𝄞b", "lines": []}"#)
    #expect(wide.length == 5 && wide.scalars(4) == 3 && wide.scalars(5) == 4)
    let combined = try carets(#"{"text": "aéb", "lines": []}"#)
    #expect(combined.after(1) == 3 && combined.before(3) == 1)
}

/// Typing in place (PLAN 3.9), as the browser's canvas types (PLAN 2.32): a caret put where a
/// press is, moved up and down the lines as the engine set them, and a word or a paragraph
/// selected by a second or a third click.
@MainActor
@Test func aPressPutsTheCaretWhereTheEngineSaysTheCharacterIs() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let typing = Typing(editor: editor)

    // Not a text: nothing to type in.
    #expect(!typing.enter("nobody", in: "cover", at: nil))
    #expect(!typing.typing)

    // The subtitle, set on two lines; a press near its start.
    let near = CGPoint(x: 130, y: 720)
    #expect(typing.enter("subtitle", in: "cover", at: near))
    #expect(typing.typing && typing.node == "subtitle" && typing.state == "cover")
    #expect(typing.told?.hasPrefix("typing in subtitle · ") == true, "\(typing.told ?? "")")
    #expect(typing.carets?.lines.count == 2)
    #expect(typing.head == 0 && typing.caret?.count == 2)
    #expect(typing.holds(near) && typing.holds(CGPoint(x: 118, y: 702), slop: 4))
    #expect(!typing.holds(CGPoint(x: 100, y: 600), slop: 4))

    // Down a line, to its end, and up again, as near the caret's x as the characters stand.
    typing.move(.line(1))
    #expect(typing.head == 34)
    typing.move(.lineEdge(start: false))
    #expect(typing.head == 62)
    typing.move(.line(-1))
    #expect(typing.head == 28)
    typing.move(.text(start: true), extend: true)
    #expect(typing.selection == NSRange(location: 0, length: 28))
    #expect(typing.covered.count == 1)

    // A second click selects the word there, a third its paragraph.
    typing.press(at: CGPoint(x: 280, y: 720), clicks: 2)
    #expect(typing.selected == "this")
    typing.press(at: CGPoint(x: 280, y: 720), clicks: 3)
    #expect(typing.selection == NSRange(location: 0, length: 62))
    // "this" is characters 4 to 8; the engine sets the "i" from x 271.7 to 282.3, the "s" to 306.1.
    typing.press(at: CGPoint(x: 266, y: 720))
    #expect(typing.head == 6 && typing.selected == nil)
    typing.drag(to: CGPoint(x: 303, y: 720))
    #expect(typing.selected == "is")

    // Another state shown stops typing.
    typing.sync(shown: "goal")
    #expect(!typing.typing && typing.told == nil)
}

/// Keys and an input method through the text input system (PLAN 3.9): each change a
/// `replace_text` by the user, the burst of typing one step to undo, a composition typed in
/// place, an accent chosen for the letter before, ⌘B's look its own step, and Escape leaving.
@MainActor
@Test func keysAndAnInputMethodTypeThroughTheTextInputSystem() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let original = editor.source
    let typing = Typing(editor: editor)
    typing.burst = .seconds(60)
    var steps: [(before: String, joins: Bool)] = []
    typing.edited = { steps.append(($0, $1)) }
    let view = CanvasKeys(typing: typing)
    view.frame = NSRect(x: 0, y: 0, width: 960, height: 540)
    view.canvas = CGSize(width: 1920, height: 1080)
    let nowhere = NSRange(location: NSNotFound, length: 0)

    #expect(typing.enter("title", in: "cover", at: nil))
    #expect(view.selectedRange() == NSRange(location: 6, length: 0))

    view.insertText(" on the Mac", replacementRange: nowhere)
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.deleteBackward(_:)))
    #expect(typing.carets?.text == "Scaena on the Ma")
    #expect(view.selectedRange() == NSRange(location: 16, length: 0))
    #expect(editor.source.contains("Scaena on the Ma"))

    // A dead key composes in place, and the letter it makes takes its place.
    view.setMarkedText("´", selectedRange: NSRange(location: 1, length: 0), replacementRange: nowhere)
    #expect(view.hasMarkedText() && view.markedRange() == NSRange(location: 16, length: 1))
    #expect(typing.carets?.text == "Scaena on the Ma´" && typing.composing.count == 1)
    view.insertText("c", replacementRange: nowhere)
    #expect(!view.hasMarkedText() && typing.carets?.text == "Scaena on the Mac")
    #expect(view.attributedSubstring(forProposedRange: NSRange(location: 7, length: 2), actualRange: nil)?.string == "on")

    // An accent chosen for the letter before replaces it.
    view.insertText("ç", replacementRange: NSRange(location: 16, length: 1))
    #expect(typing.carets?.text == "Scaena on the Maç")
    #expect(steps.count == 5 && steps.dropFirst().allSatisfy({ $0.joins }) && !steps[0].joins)
    #expect(steps[0].before == original)

    // The line's start, selected; then a word's look.
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.moveToBeginningOfLineAndModifySelection(_:)))
    #expect(typing.selected == "Scaena on the Maç")
    typing.select(NSRange(location: 7, length: 2))
    let typed = editor.source
    typing.bold()
    #expect(steps.count == 6 && !steps[5].joins && steps[5].before == typed)
    #expect(editor.source != typed, "the look is in the deck")
    #expect(typing.carets?.text == "Scaena on the Maç" && typing.selected == "on")
    #expect(typing.told?.contains("bold") == true, "\(typing.told ?? "")")

    // The burst undone in one step: the deck as it was, and the caret where it still fits.
    editor.restore(steps[0].before)
    #expect(editor.source == original)
    typing.sync(shown: "cover")
    #expect(typing.carets?.text == "Scaena" && view.selectedRange().location <= 6)

    // Escape stops typing; the canvas keeps the keys (PLAN 3.11).
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.cancelOperation(_:)))
    #expect(!typing.typing)
    #expect(view.selectedRange().location == NSNotFound)
}

/// A text's paragraphs as the deck counts them: what a list's keys act on (ADR-0018).
@Test func aListsKeysActOnTheParagraphsTheSelectionTouches() throws {
    let c = try carets(twoLines)
    #expect(c.paragraphs == [NSRange(location: 0, length: 5), NSRange(location: 6, length: 2)])
    let both = c.touched(from: 1, to: 7)
    #expect(both.first == 0 && both.last == 1)
    let second = c.touched(from: 6, to: 6)
    #expect(second.first == 1 && second.last == 1)
    #expect(c.item(0) == nil && c.item(5) == nil)
    let crlf = try carets(#"{"text": "a\r\nb", "lines": []}"#)
    #expect(crlf.paragraphs == [NSRange(location: 0, length: 1), NSRange(location: 3, length: 1)])
}

/// A list's keys (PLAN 2.69, 3.10), as a word processor's: ⌘⇧8 bullets the paragraphs the
/// selection touches, Tab moves them a level in, Return makes an item like the one it leaves, and
/// Return in an empty item ends the list there; ⌘⇧8 on bullets takes them out of it, and Tab
/// outside a list stops typing. Each is one patch, one step to undo.
@MainActor
@Test func aListsKeysMakeItemsAsAWordProcessorsDo() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let typing = Typing(editor: editor)
    var steps = 0
    typing.edited = { _, joins in
        if !joins { steps += 1 }
    }
    let view = CanvasKeys(typing: typing)
    #expect(typing.enter("subtitle", in: "cover", at: nil))
    #expect(!typing.inList && !typing.endsList)

    typing.toggle("bullet")
    #expect(typing.carets?.item(0)?.kind == "bullet" && typing.inList)
    #expect(typing.told == "subtitle: bulleted")
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.insertTab(_:)))
    #expect(typing.carets?.item(0)?.level == 1 && typing.typing)

    typing.move(.text(start: false))
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.insertNewline(_:)))
    #expect(typing.carets?.item(1)?.kind == "bullet" && typing.endsList)
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.insertNewline(_:)))
    #expect(typing.carets?.item(1) == nil && typing.carets?.item(0)?.level == 1)
    #expect(typing.told == "subtitle: the list ends")

    typing.move(.text(start: true))
    typing.toggle("bullet")
    #expect(typing.carets?.item(0) == nil && !typing.inList)
    #expect(typing.told == "subtitle: out of the list")
    view.doCommand(by: #selector(NSStandardKeyBindingResponding.insertTab(_:)))
    #expect(!typing.typing)
    #expect(steps == 5)
}

/// A text's characters (PLAN 3.10): what the inspector offers for them, a role chosen there as
/// one `style_text`, and ⌘K's link, found where the engine draws it, as a click in Play follows it.
@MainActor
@Test func charactersTakeALookAndALink() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let typing = Typing(editor: editor)
    #expect(typing.enter("title", in: "cover", at: nil))
    #expect(typing.characterChoices() == nil, "no characters selected")
    typing.select(NSRange(location: 0, length: 6))
    let offered = try #require(typing.characterChoices())
    #expect(offered.node == "title" && offered.fields.contains(where: { $0.prop == "role" }))

    // Where the characters are drawn, as the engine set them last.
    func middle() throws -> CGPoint {
        let r = try #require(typing.carets?.covered(from: 0, to: 6).first)
        return CGPoint(x: r.midX, y: r.midY)
    }

    // ⌘K: asked, then linked to a state; the link is drawn where the characters are.
    typing.askLink()
    #expect(typing.asking == "Scaena")
    #expect(!typing.link(to: "not a link!") && typing.asking == nil)
    #expect(typing.told?.contains("is no link") == true, "\(typing.told ?? "")")
    typing.askLink()
    #expect(typing.link(to: "#goal"))
    #expect(try editor.session.link(state: "cover", at: middle()) == .state("goal"))
    #expect(try editor.session.link(state: "cover", at: CGPoint(x: 10, y: 10)) == nil)
    typing.askLink()
    #expect(typing.link(to: "https://example.com/scaena"))
    #expect(try editor.session.link(state: "cover", at: middle()) == .href("https://example.com/scaena"))
    // Nothing takes the link away; dismissed, the question makes none.
    typing.askLink()
    #expect(typing.link(to: ""))
    #expect(try editor.session.link(state: "cover", at: middle()) == nil)
    typing.askLink()
    #expect(!typing.link(to: nil) && typing.told == "no link made")
    #expect(!typing.link(to: "goal"), "nothing was asked")

    // A role chosen in the inspector for the characters selected: one `style_text`.
    #expect(typing.give(["role": "headline"]))
    #expect(typing.told == "title, characters 1–6: role headline")
    let role = typing.characterChoices()?.fields.first(where: { $0.prop == "role" })
    #expect(role?.value?.string == "headline")
    #expect(typing.selection == NSRange(location: 0, length: 6))
}
