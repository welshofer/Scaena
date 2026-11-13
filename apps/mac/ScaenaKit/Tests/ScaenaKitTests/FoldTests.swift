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

/// A slide that says again what one two back said is offered to fold into that one, as the
/// browser's Story row offers it (R1's #1161): its finding names the earlier slide, the row calls
/// the fold "Fold Into Slide 10" beside the one into the slide before it, and the fold is one
/// patch, the slide's words joined to slide 10's, slide 11 left as it was.
@MainActor
@Test func aSlideSaidAgainFoldsIntoTheEarlierSlideItRepeats() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    // Slide 12 headed with slide 10's claim, a step further on.
    let again = "The deck is a projection of the story, told again."
    let said: [JSONValue] = [
        ["op": "replace", "path": "/states/31/props/headline/text", "value": .string(again)],
        ["op": "replace", "path": "/spine/sections/0/beats/11/claim", "value": .string(again)],
    ]
    _ = try editor.make(said)
    let health = try editor.session.health()
    let run = health.findings.filter { $0.slide == 12 }
    let repeat_ = try #require(run.first { $0.into != nil }, "\(run.map(\.says))")
    #expect(repeat_.kind == "story" && repeat_.into == "belief-8", "\(repeat_.says)")
    #expect(Slide.earlier(run, in: editor.slots) == [EarlierFold(into: "belief-8", label: "Fold Into Slide 10", slide: 10)])
    // Into the slide just before it, the row offers none of these: that fold is its other button.
    let before = Data(
        #"{"state":"\#(repeat_.state)","slide":12,"severity":"warning","kind":"story","says":"","ask":"","into":"belief-9"}"#
            .utf8)
    let intoEleven = try JSONDecoder().decode(Health.Finding.self, from: before)
    #expect(Slide.earlier([intoEleven], in: editor.slots).isEmpty)

    let slides = Slide.of(editor.slots).count
    let between = try editor.session.notes(state: "belief-9").text
    let folded = try editor.session.folding(repeat_.state, into: "belief-8")
    #expect(folded.refused == nil && folded.into == "belief-8" && folded.to == .place)
    #expect(folded.said == "Folded slide 12 into slide 10")
    _ = try editor.make(folded.patch)
    #expect(Slide.of(editor.slots).count == slides - 1)
    #expect(!editor.slots.contains { $0.slide == "belief-10" })
    #expect(editor.slots.contains { $0.slide == "belief-9" })
    #expect(try editor.session.notes(state: "belief-9").text == between)
}

/// The Story's fold buttons say beforehand what the fold itself will do, as the browser's do
/// (#1222): B1's slide 4 into slide 3 reads "Fold Into Slide 3" from the slots, which see a place
/// for words, but its words, laid out, would run past that place, so the fold's dry run puts them
/// in slide 3's notes and the button says so; a fold refused is held with why. Each dry run is
/// asked once for the source as it stands, and asked again once the source changes.
@MainActor
@Test func aStoryFoldSaysWhatItsDryRunWillDo() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let slots = { (state: String) in editor.slots.first { $0.state == state }?.foldsTo }
    #expect(Slide.folding("belief-2", in: editor.slots) == "Fold Into Slide 3", "the slots see a place for words")
    #expect(Slide.before("belief-2", in: editor.slots) == 3 && Slide.before("cover", in: editor.slots) == nil)

    let into = FoldAsk(state: "belief-2")
    let cover = FoldAsk(state: "cover")
    let folds = await editor.foldsAway([into, cover, into])
    #expect(folds.count == 2)
    let offer = FoldsTo.offer("Slide 3", slots: slots("belief-2"), folds[into])
    #expect(offer == FoldOffer(label: "Fold Into Slide 3's Notes", refused: nil))
    // Until the dry run answers, the slots' label stands.
    #expect(FoldsTo.offer("Slide 3", slots: slots("belief-2"), nil).label == "Fold Into Slide 3")
    // The first slide's fold is refused: held with why, under the slots' label.
    let refused = FoldsTo.offer("Slide 1", slots: nil, folds[cover])
    #expect(refused.refused == "Slide 1 has no slide before it to fold into.")

    // Asked again for the same source, each is kept; an edit asks afresh.
    let kept = editor.foldsRun.source
    _ = await editor.foldsAway([into])
    #expect(editor.foldsRun.source == kept && editor.foldsRun.folds.count == 2)
    _ = try editor.make(try editor.session.folding("goal").patch)
    let again = await editor.foldsAway([FoldAsk(state: "belief-2")])
    #expect(editor.foldsRun.source != kept && again.count == 1)
    #expect(again[into]?.to == .notes, "slide 4 is slide 3 now, its words still too many for slide 2's place")
}
