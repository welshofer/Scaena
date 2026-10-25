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

/// The Format tab's rounds of drawing asked for in a burst (lift-up 10-25c PERF-4): each outdates
/// the ones before it, which are not drawn, so a burst of edits draws once, for the last.
@Test @MainActor func aBurstOfLayoutRoundsDrawsOnlyTheLast() async throws {
    let editor = DeckEditor(session: try ScaenaSession(files: example("revenue")))
    // A round of backgrounds holds the queue first, so the three rounds of layouts asked for after
    // it all wait behind it, the last asked outdating the two before.
    let holding = Task { try await editor.backgroundsAway(state: "intro", height: 90) }
    let first = Task { try await editor.layoutsAway(state: "revenue", height: 90) }
    let second = Task { try await editor.layoutsAway(state: "revenue", height: 90) }
    let third = Task { try await editor.layoutsAway(state: "revenue", height: 90) }
    _ = try await holding.value
    let (a, b, c) = (try await first.value, try await second.value, try await third.value)
    #expect(a == nil && b == nil, "outdated rounds come back with nothing")
    #expect(c?.count == 7, "the last round is drawn")
    #expect(editor.layoutsWanted.drawn == 1, "\(editor.layoutsWanted.drawn) rounds drawn, of three asked")
}
