import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// A theme color held in the Theme panel's picker draws the slide in the theme it would make, as the
/// browser's Theme tab does (lift-up ATTR-5): the frame's digest changes while it is held, the deck
/// and its source, which the window's undo keeps, stay as they were, and letting it go draws the
/// frame as it was.
@Test func aThemeColorHeldShowsOnTheSlideBeforeItIsChosen() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    let rest = try session.digest(state: "cover")
    let source = try session.source()
    let (shown, drawn) = (editor.source, editor.drawn)
    let paper: JSONValue = ["op": "replace", "path": "/tokens/color/paper", "value": "#204060"]
    editor.show(theme: [paper])
    #expect(editor.drawn > drawn, "the canvas is drawn again")
    #expect(try session.digest(state: "cover") != rest, "the paper held is drawn")
    #expect(editor.source == shown, "nothing is made")
    #expect(try session.source() == source, "nothing is made")
    editor.show(theme: nil)
    #expect(try session.digest(state: "cover") == rest)
}

/// An Adjust Image slider held draws the picture adjusted, as the browser's does (lift-up 20261110
/// ATTR-5): the `choose` it would make previewed, nothing made, and the frame as it was once it is
/// let go.
@Test func anAdjustmentHeldShowsOnTheSlideBeforeItIsChosen() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: tortureDeck))
    editor.shown = "images"
    let session = editor.session
    let rest = try session.digest(state: "images")
    let source = try session.source()
    let shown = editor.source
    let warmer: JSONValue = [
        "op": "choose", "node": "image-cover", "prop": "adjust/warmth", "value": 0.6, "state": "images",
    ]
    editor.show([warmer])
    #expect(try session.digest(state: "images") != rest, "the warmth held is drawn")
    #expect(editor.source == shown, "nothing is made")
    #expect(try session.source() == source, "nothing is made")
    editor.still()
    #expect(try session.digest(state: "images") == rest)
}
