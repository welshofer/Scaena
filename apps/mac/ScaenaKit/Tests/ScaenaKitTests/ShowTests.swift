import Foundation
import ScaenaKit
import Testing

/// Three states: one with no cue, one whose cue runs 400 ms and holds a second, and the last.
private func slots() throws -> [ScaenaSession.Slot] {
    let json = """
        [{"state": "cover", "slide": "cover", "start": 0, "span": 0, "hold": 0},
         {"state": "why", "slide": "why", "start": 0, "span": 400, "hold": 1000},
         {"state": "end", "slide": "end", "start": 1400, "span": 400, "hold": 2000}]
        """
    return try JSONDecoder().decode([ScaenaSession.Slot].self, from: Data(json.utf8))
}

/// The browser player's pace (PLAN 2.2, 3.5): going on during a cue finishes it, then plays the
/// next state's; going back shows the state before at rest; a state that holds goes on by itself,
/// and the last waits.
@Test func aShowGoesOnAsTheBrowsersPlayerDoes() throws {
    var show = Show(slots: try slots())
    #expect(show.slot?.state == "cover" && show.playhead == Playhead())
    #expect(show.next?.state == "why")
    #expect(show.holds == nil)

    // Going on during the cue finishes it; going on at rest plays the next state's.
    show.on()
    #expect(show.slot?.state == "cover" && show.playhead == .rest)
    show.on()
    #expect(show.slot?.state == "why" && show.playhead == Playhead())
    #expect(show.holds == 1000)

    // The last holds, but nothing comes after it: it waits, and going on stays.
    show.playhead = .rest
    show.on()
    #expect(show.slot?.state == "end")
    #expect(show.holds == nil && show.next == nil)
    show.playhead = .rest
    show.on()
    #expect(show.slot?.state == "end" && show.playhead == .rest)

    // Back is the state before at rest, never past the first.
    show.back()
    #expect(show.slot?.state == "why" && show.playhead == .rest)
    show.first()
    #expect(show.slot?.state == "cover" && show.playhead == Playhead())
    show.back()
    #expect(show.slot?.state == "cover" && show.playhead == .rest)
    show.last()
    #expect(show.slot?.state == "end" && show.playhead == .rest)

    // Played from a state named, or from the first where the deck has none so named.
    #expect(Show(slots: try slots(), from: "why").slot?.state == "why")
    #expect(Show(slots: try slots(), from: "gone").slot?.state == "cover")
    #expect(Show(slots: []).slot == nil)
}
