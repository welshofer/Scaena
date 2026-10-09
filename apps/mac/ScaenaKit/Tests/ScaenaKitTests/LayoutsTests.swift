import Foundation
import ScaenaKit
import Testing

/// The revenue example's bundle: its deck, and the fonts, themes, data, and pictures beside it.
private func revenueExample() throws -> [String: Data] {
    let examples = repository.appending(path: "docs/examples")
    var files = ["deck.json": try Data(contentsOf: examples.appending(path: "revenue.deck.json"))]
    for dir in ["fonts", "themes", "data", "assets"] {
        let root = examples.appending(path: dir).resolvingSymlinksInPath()
        guard let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey])
        else { continue }
        for case let url as URL in walk where (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true {
            let path = url.resolvingSymlinksInPath().pathComponents.dropFirst(root.pathComponents.count)
            files["\(dir)/\(path.joined(separator: "/"))"] = try Data(contentsOf: url)
        }
    }
    return files
}

/// The layouts a slide may take (PLAN 2.92, 3.26), as the Format tab shows them: judged a step at
/// a time, best first, each drawn small and its picture taken once, and one chosen by its patch.
@Test func aSlidesLayoutsAreJudgedAStepAtATimeAndDrawnSmall() throws {
    let session = try ScaenaSession(files: revenueExample())
    let count = try session.layoutsBegin(state: "revenue")
    var steps = 1
    while try session.layoutsStep() { steps += 1 }
    #expect(steps == count, "each step judges one layout")
    let drawn = try session.layoutsDrawn(height: 90)
    // The layout it takes now first, nothing found in it; the others, each with an error.
    #expect(drawn.map(\.suggestion.layout).first == "figure" && drawn.count == 3)
    #expect(drawn.first.map { $0.suggestion.current && $0.suggestion.errors == 0 } == true)
    #expect(drawn.dropFirst().allSatisfy { !$0.suggestion.current && $0.suggestion.errors > 0 })
    #expect(drawn.allSatisfy { $0.image.width == 160 && $0.image.height == 90 })
    #expect(throws: ScaenaError.self) { try session.layoutPicture(0) }
    #expect(throws: ScaenaError.self) { try session.layoutsStep() }

    // Another chosen by its patch: the slide takes it.
    let other = drawn[1].suggestion
    #expect(try session.tool("deck_patch", ["ops": .array(other.patch)]).edited)
    let layout = try session.stateChoices(state: "revenue").fields.first { $0.prop == "layout" }
    #expect(layout?.value?.string == other.layout)
}
