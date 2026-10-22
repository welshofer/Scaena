import Foundation
import ScaenaKit
import Testing

/// A slide's background (PLAN 2.148, 3.32), as the Format tab offers it with nothing selected: None,
/// then each of the theme's presets, each drawn small; one chosen is one patch, and its undo gives
/// the slide back as it was. A slide with a chart says why backgrounds stay off it.
@Test @MainActor func aSlidesBackgroundIsChosenAndUndone() throws {
    let editor = DeckEditor(session: try ScaenaSession(files: example("revenue")))
    let (offered, images) = try editor.session.backgroundsDrawn(state: "intro", height: 90)
    #expect(offered.first == "intro" && offered.held == nil)
    #expect(offered.options.map(\.label) == ["None", "Backdrop", "Texture"])
    #expect(offered.options[1].said == "Mesh Gradient")
    // Its own mesh is no preset's: none is the slide's now.
    #expect(offered.options.allSatisfy { !$0.current })
    #expect(images.allSatisfy { $0?.height == 90 })
    // A background moves: its picture a moment on is another.
    let (still, on) = (try editor.session.backgroundPicture(1, at: 0), try editor.session.backgroundPicture(1, at: 2))
    #expect(still.rgba != on.rgba)

    // Backdrop chosen: the slide takes it, and no other.
    let backdrop = try #require(offered.options.first { $0.preset == "backdrop" })
    let before = try editor.make(backdrop.patch)
    let after = try editor.session.backgrounds(state: "intro", height: 90)
    #expect(after.options.first { $0.current }?.preset == "backdrop")
    #expect(try editor.session.backgrounds(state: "close", height: 90).options.allSatisfy { !$0.current })

    // Undone, the slide is as it was.
    #expect(editor.restore(before) != nil)
    #expect(try editor.session.backgrounds(state: "intro", height: 90).options.allSatisfy { !$0.current })

    // A slide with a chart: each dimmed, and why; None still taken.
    let data = try editor.session.backgrounds(state: "mix", height: 90)
    #expect(data.first == "revenue" && data.held != nil)
    #expect(data.options.filter { data.offers($0) }.map(\.label) == ["None"])
}
