import Foundation
import ScaenaKit
import Testing

/// How many pixels of `b` differ from `a` in the foot's margin (under Dusk's last row, 96 of 1080)
/// and above it.
private func differ(_ a: ScaenaSession.Pixels, _ b: ScaenaSession.Pixels) -> (foot: Int, above: Int) {
    let foot = Int(Double(a.height) * (1 - 96.0 / 1080))
    var out = (foot: 0, above: 0)
    for y in 0..<a.height {
        for x in 0..<a.width {
            let i = (y * a.width + x) * 4
            let d = (0..<3).reduce(0) { $0 + abs(Int(a.rgba[i + $1]) - Int(b.rgba[i + $1])) }
            guard d >= 24 else { continue }
            if y < foot { out.above += 1 } else { out.foot += 1 }
        }
    }
    return out
}

/// A slide's number and footer from the inspector (PLAN 2.103, ADR-0028), as the browser's: the
/// deck's Slide Numbers and Footer each one `furnish`, the slide's say one `set_state` on its
/// first state, each drawn at the slide's foot and nowhere else, and each one step to undo.
@Test func aSlidesNumberAndFooterAreTheDecksAndEachSlidesSay() throws {
    let editor = DeckEditor(session: try ScaenaSession(files: example("revenue")))
    editor.shown = "mix"
    let original = editor.source
    let none = try #require(try editor.session.stateChoices(state: "mix").furniture)
    #expect(none.placed && !none.number && none.footer == nil && none.slide == nil && !none.shown)
    #expect(none.head == "revenue", "a step's slide says it on its first state")
    let bare = try editor.session.pixels("mix", width: 480)

    // Numbered: the deck's header says so, and the slide draws its number at its foot alone.
    #expect(try editor.make(FurnitureChoice.numbering(true)) == original)
    #expect(editor.source.contains("furniture:{number: true}"))
    let numbered = try #require(try editor.session.stateChoices(state: "mix").furniture)
    #expect(numbered.number && numbered.shown)
    let drawn = differ(bare, try editor.session.pixels("mix", width: 480))
    #expect(drawn.foot > 0 && drawn.above == 0, "drawn at the foot: \(drawn)")

    // A footer, then the slide hides both, from its step: written on `revenue`.
    try editor.make(FurnitureChoice.footing("  Q3 Review · Board "))
    #expect(editor.source.contains("footer: \"Q3 Review · Board\""))
    try editor.make(numbered.showing(false))
    #expect(editor.source.contains("state revenue") && editor.source.contains("furniture:false"))
    let hidden = try #require(try editor.session.stateChoices(state: "mix").furniture)
    #expect(hidden.slide == false && !hidden.shown)
    let gone = differ(bare, try editor.session.pixels("mix", width: 480))
    #expect(gone.foot == 0 && gone.above == 0, "hidden, the slide draws as it did: \(gone)")

    // No words take the footer away; as its layout has them takes the slide's say away.
    try editor.make(FurnitureChoice.footing(" "))
    #expect(!editor.source.contains("footer:"))
    try editor.make(hidden.showing(nil))
    #expect(!editor.source.contains("furniture:false"))
    #expect(throws: ScaenaError.self) { try editor.make(FurnitureChoice.footing("")) }
}

/// A footer left with a space added, or a space typed in an empty one, changes nothing: no patch is
/// made, so nothing fails, and the deck stays as it was. Other words set it, trimmed.
@Test func aFooterLeftAsItWasMakesNoPatch() throws {
    let editor = DeckEditor(session: try ScaenaSession(files: example("revenue")))
    editor.shown = "mix"
    let none = try #require(try editor.session.stateChoices(state: "mix").furniture)
    #expect(none.footing(to: " ") == nil, "a space in an empty footer")
    try editor.make(FurnitureChoice.footing("Q3 Review · Board"))
    let footed = try #require(try editor.session.stateChoices(state: "mix").furniture)
    let source = editor.source
    #expect(footed.footing(to: "Q3 Review · Board ") == nil, "a space added")
    #expect(editor.source == source)
    let other = try #require(footed.footing(to: " Board "))
    try editor.make(other)
    #expect(editor.source.contains("footer: \"Board\""))
}
