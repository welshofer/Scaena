import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// What several nodes share in the inspector (PLAN 2.42, 3.13): each field every one offers, its
/// value where they all show it alike, and where it lives where they agree.
@Test func severalShareWhatEachOffers() throws {
    let json = #"""
        [{"node": "a", "type": "text", "state": "s", "fields": [
           {"prop": "role", "takes": {"kind": "name", "of": "text-role", "names": ["body", "title"]}, "value": "body", "lives": "node"},
           {"prop": "style/color", "takes": {"kind": "name", "of": "color", "names": ["ink", "accent"]}, "value": "ink", "lives": {"state": "s"}},
           {"prop": "maxLines", "takes": {"kind": "number", "min": 1, "whole": true}}]},
         {"node": "b", "type": "text", "state": "s", "fields": [
           {"prop": "role", "takes": {"kind": "name", "of": "text-role", "names": ["body", "title"]}, "value": "body", "lives": "node"},
           {"prop": "style/color", "takes": {"kind": "name", "of": "color", "names": ["ink", "accent"]}, "value": "accent", "lives": "node"}]}]
        """#
    let all = try JSONDecoder().decode([Choices].self, from: Data(json.utf8))
    let shared = Field.shared(all)
    #expect(shared.map(\.prop) == ["role", "style/color"])
    #expect(shared[0].value == "body" && shared[0].lives == .node)
    #expect(shared[1].value == nil && shared[1].lives == nil, "they differ: neither shows")
    #expect(Field.shared([]).isEmpty)
}

/// Several selected through the editor (PLAN 3.13), as the browser's canvas takes them (PLAN 2.42,
/// 2.43): moved together, aligned, ordered, grouped where they stand, and taken apart; each one
/// patch.
@Test func severalMoveArrangeAndGroupThroughTheEditor() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "goal-bar"
    let session = editor.session
    let nodes = ["kicker", "headline", "body"]

    let moved = try #require(try session.together(state: "goal-bar", nodes: nodes, by: CGVector(dx: 200, dy: 0), free: true))
    #expect(moved.landed.map(\.node).sorted() == nodes.sorted() && !moved.patch.isEmpty)
    try editor.make(moved.patch)

    if let aligned = try session.arranging(state: "goal-bar", nodes: nodes, how: .align("left")), !aligned.patch.isEmpty {
        try editor.make(aligned.patch)
        let lefts = try session.boxes(state: "goal-bar").filter { nodes.contains($0.node) }.map { $0.rect[0] }
        #expect(Set(lefts.map { ($0 * 10).rounded() }).count == 1, "aligned on one left edge: \(lefts)")
    }
    _ = try session.arranging(state: "goal-bar", nodes: nodes, how: .order("front"))

    let grouping = try session.grouping(state: "goal-bar", nodes: nodes)
    try editor.make(grouping.patch)
    #expect(try session.boxes(state: "goal-bar").filter { $0.parent == grouping.id }.count == 3)
    try editor.make([["op": "ungroup", "group": .string(grouping.id)]])
    #expect(try session.boxes(state: "goal-bar").allSatisfy { $0.parent != grouping.id })
}
