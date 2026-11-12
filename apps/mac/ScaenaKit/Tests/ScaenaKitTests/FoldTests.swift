import Foundation
@testable import ScaenaKit
import Testing

/// A slide folded into the one before it (editor dogfood 6), as the browser folds one: each slot
/// says where its slide's words would go, the command is named so before it is made, and the fold
/// is one patch the editor takes as one step, the slide gone and said in a person's words.
@MainActor
@Test func aSlideFoldsIntoTheOneBeforeItSaidBeforeAndAfter() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let to = { (state: String) in editor.slots.first { $0.state == state }?.foldsTo }
    // The cover takes no words, so the goal's go to its notes; a belief takes the next one's.
    #expect(to("cover") == nil)
    #expect(to("goal") == .notes && to("goal-bar") == .notes)
    #expect(to("belief-2") == .place)
    #expect(Slide.folding("cover", in: editor.slots) == nil)
    #expect(Slide.folding("goal-why", in: editor.slots) == "Fold Into Slide 1's Notes")
    #expect(Slide.folding("belief-2", in: editor.slots) == "Fold Into Slide 3")

    let slides = Slide.of(editor.slots).count
    let folded = try editor.session.folding("goal")
    #expect(folded.refused == nil && folded.to == .notes && folded.into == "cover")
    #expect(folded.said == "Folded slide 2 into slide 1's notes")
    _ = try editor.make(folded.patch)
    #expect(Slide.of(editor.slots).count == slides - 1)
    #expect(!editor.slots.contains { $0.state == "goal" })
    let notes = try editor.session.notes(state: "cover").text
    #expect(notes.hasPrefix("Build the tool we would design today"), "\(notes)")

    // The first slide has none before it: refused, saying why, with no patch.
    let first = try editor.session.folding("cover")
    #expect(first.refused == "Slide 1 has no slide before it to fold into." && first.patch.isEmpty)
}
