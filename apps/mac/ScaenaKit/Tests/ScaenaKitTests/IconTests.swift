import Foundation
import ScaenaKit
import SwiftUI
import Testing

/// The icons that ship, found and inserted as the browser's picker finds and inserts them (PLAN
/// 2.104): found by their words, each weight its own path on one view, drawn in SwiftUI by that
/// view, and inserted as one patch, a path shape on its view named by the icon, one step to undo.
@Test func anIconIsFoundByItsWordsAndInsertedAsOnePatch() throws {
    let editor = DeckEditor(session: try ScaenaSession(files: example("revenue")))
    editor.shown = "revenue"

    // Found by words: the name asked whole first, at most as many as asked, and how many in all.
    let up = try editor.session.icons("arrow up", limit: 5)
    #expect(up.icons.first?.name == "arrow-up")
    #expect(up.icons.count == 5 && up.found > 5, "\(up.found)")
    #expect(try editor.session.icons("zzqx").icons.isEmpty)

    // Each weight its own path, on one view.
    let outline = try #require(try editor.session.icons("heart", limit: 1).icons.first).icon
    let solid = try #require(try editor.session.icons("heart", fill: true, limit: 1).icons.first).icon
    #expect(outline.name == "heart" && outline.path != solid.path)
    #expect(outline.view == [0, 0, 256, 256] && solid.view == outline.view)

    // Drawn by its view in a tall rect: as wide as the rect at most, and centered in its height.
    let drawn = IconShape(outline).path(in: CGRect(x: 0, y: 0, width: 100, height: 200)).boundingRect
    #expect(drawn.width > 50 && drawn.maxX <= 100.5, "\(drawn)")
    #expect(drawn.minY >= 49.5 && drawn.maxY <= 150.5, "\(drawn)")

    // Inserted: one patch, a path shape on the icon's view, named by it, one step to undo.
    let original = editor.source
    let added = try editor.session.insertingIcon(state: "revenue", icon: outline, at: CGPoint(x: 960, y: 540))
    #expect(added.id == "heart")
    #expect(try editor.make(added.patch) == original)
    #expect(editor.source.contains("heart") && editor.source.contains("view:"))
    let again = try editor.session.insertingIcon(state: "revenue", icon: outline, at: CGPoint(x: 400, y: 300))
    #expect(again.id == "heart-2", "a second heart takes a name of its own")
}
