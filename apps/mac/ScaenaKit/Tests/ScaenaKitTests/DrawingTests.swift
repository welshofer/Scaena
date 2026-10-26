import CoreGraphics
import Foundation
@testable import ScaenaKit
import Testing

/// The slides are drawn small away from the window (lift-up PERF-2): drawn on a queue of their
/// own from the deck as it stands, the pixels the session paints here, each width of a state kept
/// beside the others, so the presenter's next and a remote's slide no longer draw each other out,
/// and a remote's made a PNG there too.
@Test @MainActor func aStateIsDrawnAwayFromTheWindowAsHereAndEachWidthIsKept() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    #expect(editor.valid)

    // Drawn away, it is what the session paints here.
    let away = try #require(await editor.drawingAway("goal", width: 160))
    let here = try editor.session.pixels("goal", width: 160)
    #expect((away.width, away.height) == (here.width, here.height))
    #expect(away.dataProvider?.data as Data? == here.rgba, "drawn away as the session paints it")

    // Two widths of one state stand side by side, each kept: the presenter's next and a remote's.
    let next = try #require(await editor.drawingAway("goal", width: 360))
    let remote = try #require(await editor.drawingAway("goal", width: 960))
    #expect(await editor.drawingAway("goal", width: 360) === next, "the presenter's next is kept")
    #expect(await editor.drawingAway("goal", width: 960) === remote, "and a remote's beside it")
    #expect(editor.drawing("goal", width: 160) === away, "drawn here, the drawing kept is taken")

    // An edit to the state draws it again; one to another state leaves it.
    editor.shown = "cover"
    let typed: JSONValue = [
        "op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Edited. ",
    ]
    try editor.make([typed])
    #expect(await editor.drawingAway("goal", width: 360) === next, "an edit elsewhere leaves it")
    let cover = try #require(await editor.drawingAway("cover", width: 160))
    #expect(cover.dataProvider?.data as Data? == (try editor.session.pixels("cover", width: 160)).rgba)

    // A remote's slide is a PNG of it.
    let png = try #require(await editor.pngAway("goal", width: 960))
    #expect(png.starts(with: [0x89, 0x50, 0x4E, 0x47]), "a PNG")
}

/// The cue a show goes on to is laid out ahead away from the window (lift-up 10-25b PERF-5): the
/// session takes it, and its frames are those the session lays out here; once its cue is kept, or
/// the deck changed since, another is not taken.
@Test @MainActor func aCueLaidOutAwayFromTheWindowIsTheOneItsFramesSample() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let cued = try #require(editor.slots.dropFirst().first { $0.span > 0 })
    let here = try ScaenaSession(directory: b1).frame(cued.state, at: cued.span / 2)

    await editor.preparing(cued.state)
    #expect(try editor.session.frame(cued.state, at: cued.span / 2) == here, "sampled as laid out here")
    let laying = try #require(editor.session.laying())
    let again = try laying.cue(format: nil, state: cued.state)
    #expect(!editor.session.cue(laid: again), "the cue shown is kept")

    let stale = try laying.cue(format: nil, state: cued.state)
    let typed: JSONValue = [
        "op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Edited. ",
    ]
    editor.shown = "cover"
    try editor.make([typed])
    #expect(!editor.session.cue(laid: stale), "the deck changed since")
}
