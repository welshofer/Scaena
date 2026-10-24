import CoreGraphics
import Foundation
import ScaenaKit
import ScaenaNotes
import ScaenaRemote
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
/// and the last waits; going on past it, the show is over (PLAN 2.120).
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

    // The last holds, but nothing comes after it: it waits, and going on past it ends the show,
    // which going back takes back to the last at rest.
    show.playhead = .rest
    show.on()
    #expect(show.slot?.state == "end")
    #expect(show.holds == nil && show.next == nil && !show.over)
    show.playhead = .rest
    show.on()
    #expect(show.slot?.state == "end" && show.playhead == .rest && show.over)
    show.back()
    #expect(show.slot?.state == "end" && show.playhead == .rest && !show.over)

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

/// A hold runs as the browser's player runs one (PLAN 2.105): the screen black or white stops it
/// where it stands, and the slide back runs on with what was left of it; a slide's number the deck
/// lacks leaves it running; and the slide back where no hold ran, as after going back, runs none.
@Test func aHoldStopsUnderABlackScreenAndRunsOnWithWhatWasLeft() throws {
    let start = Date(timeIntervalSinceReferenceDate: 0)
    let at = { (seconds: Double) in start.addingTimeInterval(seconds) }
    var show = Show(slots: try slots(), from: "why")
    show.playhead = .rest
    let hold = show.rested(at: start)
    #expect(hold == 1000)
    #expect(show.holding(at: at(0.25)) == 750)
    show.blanking(.black, at: at(0.25))
    #expect(show.holding(at: at(5)) == nil, "no hold runs under a black screen")
    show.blanking(.white, at: at(6))
    #expect(show.holding(at: at(7)) == nil, "nor under a white one")
    show.unblank(at: at(10))
    #expect(show.holding(at: at(10)) == 750, "the slide back runs on with what was left")
    #expect(show.holding(at: at(10.5)) == 250)

    // A number the deck lacks leaves the show as it was, its hold running.
    show.type(9)
    let nowhere = show.goTyped()
    #expect(!nowhere && show.holding(at: at(10.5)) == 250)

    // Back onto a state that holds, it waits to be told: the slide back after a black screen runs
    // no hold there, where none ran.
    show.on()
    #expect(show.slot?.state == "end" && show.holding(at: at(11)) == nil)
    show.back()
    #expect(show.slot?.state == "why" && show.holding(at: at(11)) == nil)
    show.blanking(.black, at: at(12))
    show.unblank(at: at(13))
    #expect(show.holding(at: at(13)) == nil, "the slide back starts no hold that was not running")
}

/// A touch held as the laser (PLAN 2.105): it points while it is down, and its lifting turns no
/// slide for half a second after; one whose gesture is cancelled, as when its view goes, points no
/// more, and the next tap turns a slide.
@Test func aTouchHeldAsTheLaserIsLetGoWhenItsGestureIsCancelled() {
    let start = Date(timeIntervalSinceReferenceDate: 0)
    var touch = HeldTouch()
    #expect(!touch.lifting(at: start) && touch.at == nil)
    touch.hold(CGPoint(x: 0.5, y: 0.5))
    #expect(touch.down && touch.at == CGPoint(x: 0.5, y: 0.5) && touch.lifting(at: start))
    touch.lift(at: start)
    #expect(touch.at == nil && touch.lifting(at: start.addingTimeInterval(0.25)))
    #expect(!touch.lifting(at: start.addingTimeInterval(1)))
    touch.hold(CGPoint(x: 0.25, y: 0.75))
    touch.cancel()
    #expect(!touch.down && touch.at == nil && !touch.lifting(at: start.addingTimeInterval(5)))
}

/// The end of the show (PLAN 2.120), as Keynote's and PowerPoint's: going on past the last state at
/// rest, and only then, the show is over; going back shows the last state again; and the first, the
/// last, a link, a slide's number, or a black screen each put the end away for what they show.
@Test func goingOnPastTheLastStateEndsTheShow() throws {
    var show = Show(slots: try slots(), from: "end")
    // During the last state's cue, going on finishes it: the show is not over yet.
    show.on()
    #expect(show.slot?.state == "end" && show.playhead == .rest && !show.over)
    show.on()
    #expect(show.over && show.slot?.state == "end")
    // Going on again stays over: leaving is the window's to do.
    show.on()
    #expect(show.over && show.slot?.state == "end")
    show.back()
    #expect(!show.over && show.slot?.state == "end" && show.playhead == .rest)

    show.on()
    show.first()
    #expect(!show.over && show.slot?.state == "cover")
    show.last()
    show.on()
    show.last()
    #expect(!show.over && show.slot?.state == "end")
    show.on()
    let went = show.go(to: "why")
    #expect(went && !show.over && show.slot?.state == "why")
    show.last()
    show.on()
    show.type(1)
    let typed = show.goTyped()
    #expect(typed && !show.over && show.slot?.state == "cover")
    show.last()
    show.on()
    show.blanking(.black)
    #expect(!show.over && show.blank == .black)
    show.unblank()
    #expect(!show.over && show.slot?.state == "end")

    // A deck with no states has no end to come to.
    var none = Show(slots: [])
    none.on()
    none.on()
    #expect(!none.over)
}

/// The presenter's timer (PLAN 2.121): it counts from when it began, pauses where it stands and runs
/// on from there, and goes back to 0:00 running or paused as it was.
@Test func thePresentersTimerPausesAndResets() {
    let start = Date(timeIntervalSinceReferenceDate: 0)
    let at = { (seconds: Double) in start.addingTimeInterval(seconds) }
    var timer = Stopwatch(at: start)
    #expect(!timer.paused && timer.elapsed(at: at(65)) == 65)
    #expect(timer.reads(at: at(65.9)) == "1:05")
    timer.toggle(at: at(70))
    #expect(timer.paused && timer.elapsed(at: at(100)) == 70, "paused, it stands still")
    timer.toggle(at: at(100))
    #expect(!timer.paused && timer.elapsed(at: at(105)) == 75, "and runs on from where it stood")
    timer.reset(at: at(110))
    #expect(!timer.paused && timer.elapsed(at: at(110)) == 0 && timer.elapsed(at: at(112)) == 2)
    #expect(timer.reads(at: at(112)) == "0:02")
    timer.toggle(at: at(120))
    timer.reset(at: at(130))
    #expect(timer.paused && timer.elapsed(at: at(200)) == 0, "reset while paused, it waits at 0:00")
    #expect(timer.reads(at: at(200)) == "0:00")
    timer.toggle(at: at(200))
    #expect(timer.elapsed(at: at(3_725)) == 3_525 && timer.reads(at: at(3_725)) == "58:45")
}

/// The notes a presenter reads (SPEC §2.6, lift-up STAB-1): a state's own, else those of the
/// first beat that names it, which a generated deck writes them on, so the presenter's window, the
/// remote, and a rehearsal read what the deck has.
@Test func aPresenterReadsAStatesOwnNotesElseItsBeats() throws {
    let session = try ScaenaSession(files: example("revenue"))
    // `revenue` has none of its own; the beat `doubled` names it.
    #expect(try session.notes(state: "revenue").text.hasPrefix("Pause on the Q3 bar before the build."))
    // `intro`'s own win over its beat's.
    #expect(try session.notes(state: "intro").text == "Open on the title. Don't talk over the build.")
    #expect(throws: ScaenaError.self) { try session.notes(state: "no-such-state") }
}

/// The notes as a speaker reads them (SPEC §2.6, lift-up USE-1): the markdown a generated deck's
/// model writes is kept as written, and the session reads it as blocks, a list and a table with the
/// words in their looks, which `NotesText` shows on the Mac, the iPad, and the remote; plain notes
/// are their lines.
@Test func notesAreReadAsASpeakerReadsThem() throws {
    let session = try ScaenaSession(files: example("revenue"))
    let plain = try session.notes(state: "intro")
    #expect(plain.blocks == [.lines([[NoteSpan("Open on the title. Don't talk over the build.")]])])
    let written = "Lead with the **number**.\n- Pro is a third\n- Then the ask\n\n| Tier | Share |\n|---|---|\n| Pro | 33% |"
    let op: JSONValue = ["op": "set_state", "id": "mix", "prop": "notes", "value": .string(written)]
    #expect(try session.tool("deck_patch", ["ops": .array([op])]).edited)
    let read = try session.notes(state: "mix")
    #expect(read.text == written)
    #expect(
        read.blocks == [
            .lines([[NoteSpan("Lead with the "), NoteSpan("number", bold: true), NoteSpan(".")]]),
            .list(ordered: false, items: [NoteItem(spans: [NoteSpan("Pro is a third")]), NoteItem(spans: [NoteSpan("Then the ask")])]),
            .table(header: true, rows: [[[NoteSpan("Tier")], [NoteSpan("Share")]], [[NoteSpan("Pro")], [NoteSpan("33%")]]]),
        ], "\(read.blocks)")
    #expect(read.blocks.plain == "Lead with the number.\n• Pro is a third\n• Then the ask\nTier · Share\nPro · 33%")
    // A remote is sent the blocks with the words, and one sent none reads the words as lines.
    let place = RemotePlace(deck: "Q3", showing: true, state: "mix", notes: read.text, read: read.blocks)
    let sent = try JSONDecoder().decode(RemotePlace.self, from: try JSONEncoder().encode(place))
    #expect(sent == place && sent.read == read.blocks)
    let older = try JSONDecoder().decode(
        RemotePlace.self, from: Data(#"{"deck":"Q3","showing":true,"state":"mix","index":0,"count":1,"ms":0,"rest":true,"notes":"a\nb"}"#.utf8))
    #expect(older.read.isEmpty && NoteBlock.of(older.notes) == [.lines([[NoteSpan("a")], [NoteSpan("b")]])])
}

/// The presenter's notes step up and down the sizes a speaker reads at (lift-up USE-4), as the
/// browser's presenter view steps them: held at either end, and a size out of the list stepping
/// from the next in it.
@Test func thePresentersNotesStepThroughTheBrowsersSizes() {
    #expect(NotesSize.stepped(20, by: 1) == 24)
    #expect(NotesSize.stepped(24, by: -1) == 20)
    #expect(NotesSize.stepped(48, by: 1) == 48, "held at the largest")
    #expect(NotesSize.stepped(12, by: -1) == 12, "held at the smallest")
    #expect(NotesSize.stepped(15, by: 1) == 18, "15 steps from 16, the next in the list")
    #expect(NotesSize.stepped(200, by: -1) == 40)
    #expect(NotesSize.sizes.contains(NotesSize.standard))
}
