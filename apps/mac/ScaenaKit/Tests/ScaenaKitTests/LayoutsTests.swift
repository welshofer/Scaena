import Foundation
import ScaenaKit
import Testing

/// The layouts a slide may take (PLAN 2.92, 3.26), as the Format tab shows them: judged a step at
/// a time, best first, each drawn small and its picture taken once, and one chosen by its patch.
@Test func aSlidesLayoutsAreJudgedAStepAtATimeAndDrawnSmall() throws {
    let session = try ScaenaSession(files: example("revenue"))
    let count = try session.layoutsBegin(state: "revenue")
    var steps = 1
    while try session.layoutsStep() { steps += 1 }
    #expect(steps == count, "each step judges one layout")
    let drawn = try session.layoutsDrawn(height: 90)
    // The layout it takes now first, nothing found in it; the others, each with an error.
    #expect(drawn.map(\.suggestion.layout).first == "figure" && drawn.count == 7)
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

/// The theme's layouts to choose from (PLAN 3.31), as the picker lists them before it draws any,
/// each named as a person calls it, in its section; then a few drawn as the picker asks for them,
/// kept by the editor, and drawn again only for another theme or format.
@Test @MainActor func theLayoutsAreListedThenDrawnAFewAtATimeAndKept() throws {
    let session = try ScaenaSession(files: example("revenue"))
    let listed = try session.starters()
    #expect(listed.count == 164 && listed.last?.layout == nil, "Dusk's 163, then a blank slide")
    #expect(listed.allSatisfy { $0.width == 0 && $0.height == 0 }, "listed, none drawn")
    let swot = listed.first { $0.layout == "swot" }
    #expect(swot?.name == "SWOT" && swot?.group == "Frameworks")
    #expect(swot?.matches("strengths threats") == true && swot?.matches("receipt") == false)
    // Found by another name people call it (ADR-0031): Kanban opens Roadmap, and Roadmap alone.
    #expect(listed.filter { $0.matches("kanban") }.map(\.layout) == ["roadmap"])
    let sections = listed.compactMap(\.group).reduce(into: [String]()) { if !$0.contains($1) { $0.append($1) } }
    #expect(sections.count == 16 && sections.first == "Titles" && sections.last == "Closing")

    // Those asked for, in the order asked, a name the theme lacks passed over.
    let some = try session.startersDrawn(after: "revenue", height: 90, only: ["swot", "nowhere", nil])
    #expect(some.map(\.starter.layout) == ["swot", nil])
    #expect(some.allSatisfy { $0.image.height == 90 })

    // The editor keeps what it drew: asked again, the same pictures, none drawn anew.
    let editor = DeckEditor(session: session)
    #expect(editor.startersKept(["swot"], height: 90).isEmpty)
    let first = editor.starters(["swot", "okrs"], after: "revenue", height: 90)
    #expect(first.map(\.starter.layout) == ["swot", "okrs"])
    let kept = editor.startersKept(["okrs", "swot", "raci"], height: 90)
    #expect(kept.map(\.starter.layout) == ["okrs", "swot"], "only those drawn, in the order asked")
    #expect(kept.last?.image === first.first?.image)
    #expect(editor.startersKept(["swot"], height: 120).isEmpty, "another height draws them again")
}

/// A layout's picture is marked only where the slide laid out in it would leave more to put right
/// than it has now (PLAN 3.26): the layout it takes marks nothing, whatever lint finds in it, since
/// the slide marks that itself; one as good marks nothing either.
@Test func aLayoutIsMarkedOnlyWhereItWouldLeaveMoreToPutRight() throws {
    let suggestion = { (json: String) in try JSONDecoder().decode(LayoutSuggestion.self, from: Data(json.utf8)) }
    let now = try suggestion(
        #"{"layout":"title","current":true,"errors":0,"warnings":2,"reach":[],"patch":[],"width":160,"height":90}"#)
    let alike = try suggestion(
        #"{"layout":"centered","errors":0,"warnings":2,"reach":[],"patch":[],"width":160,"height":90}"#)
    let worse = try suggestion(
        #"{"layout":"photo","errors":1,"warnings":1,"reach":[],"patch":[],"width":160,"height":90}"#)
    let better = try suggestion(
        #"{"layout":"plain","errors":0,"warnings":0,"reach":[],"patch":[],"width":160,"height":90}"#)
    #expect(now.adds(over: now).errors == 0 && now.adds(over: now).warnings == 0)
    #expect(alike.adds(over: now).errors == 0 && alike.adds(over: now).warnings == 0)
    #expect(worse.adds(over: now).errors == 1 && worse.adds(over: now).warnings == 0)
    #expect(better.adds(over: now).errors == 0 && better.adds(over: now).warnings == 0)
    #expect(worse.adds(over: nil).errors == 1 && worse.adds(over: nil).warnings == 1)

    // Revenue's: the layout it takes finds nothing, and each other an error more.
    let session = try ScaenaSession(files: example("revenue"))
    _ = try session.layoutsBegin(state: "revenue")
    while try session.layoutsStep() {}
    let drawn = try session.layoutsDrawn(height: 90).map(\.suggestion)
    let current = drawn.first { $0.current }
    #expect(current?.adds(over: current).errors == 0)
    #expect(drawn.filter { !$0.current }.allSatisfy { $0.adds(over: current).errors > 0 })
}
