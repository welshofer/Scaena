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

/// A link goes to a state, its cue played (PLAN 2.70, 3.10); one the deck lacks goes nowhere.
@Test func aLinkGoesToAStateItsCuePlayed() throws {
    var show = Show(slots: try slots())
    // `go` mutates the show: called outside `#expect`, which takes what it checks as immutable.
    let went = show.go(to: "end")
    #expect(went)
    #expect(show.slot?.state == "end" && show.playhead == Playhead())
    let nowhere = show.go(to: "nowhere")
    #expect(!nowhere)
    #expect(show.slot?.state == "end")
}

/// The presenter's keys (PLAN 2.105), as the browser's player's: the screen black or white holds a
/// cue where it stands and runs no hold under it, and the same key, or the slide again, plays it on
/// from there; a slide's number typed, then Return, goes there, its cue played and the slide back,
/// and one the deck lacks goes nowhere, the number going either way.
@Test func theScreenGoesBlackAndASlidesNumberGoesThere() throws {
    var show = Show(slots: try slots(), from: "why")
    show.playhead = Playhead(ms: 120, playing: true)
    show.blanking(.black)
    #expect(show.blank == .black && show.playhead == Playhead(ms: 120, playing: false))
    #expect(show.holds == nil, "no hold runs under a black screen")
    show.blanking(.white)
    #expect(show.blank == .white && !show.playhead.playing, "black to white, the cue still held")
    show.blanking(.white)
    #expect(show.blank == nil && show.playhead == Playhead(ms: 120, playing: true), "the cue plays on from where it stood")
    #expect(show.holds == 1000)
    // At rest, the slide comes back at rest.
    show.playhead = .rest
    show.blanking(.black)
    show.unblank()
    #expect(show.blank == nil && show.playhead == .rest)

    // A slide's number: no slide begins with 0; digits run on, and the last is taken back.
    #expect(show.slides == 3)
    show.type(0)
    #expect(show.typed.isEmpty)
    show.type(1)
    show.type(3)
    #expect(show.typed == "13")
    show.untype()
    #expect(show.typed == "1")
    show.blanking(.black)
    // `goTyped` mutates the show: called outside `#expect`.
    let went = show.goTyped()
    #expect(went && show.slot?.state == "cover" && show.playhead == Playhead() && show.blank == nil)
    #expect(show.typed.isEmpty)
    show.type(9)
    let nowhere = show.goTyped()
    #expect(!nowhere && show.slot?.state == "cover" && show.typed.isEmpty)
    show.type(3)
    show.untype(all: true)
    #expect(show.typed.isEmpty)
}
