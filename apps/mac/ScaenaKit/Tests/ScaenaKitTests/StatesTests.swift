import Foundation
import ScaenaKit
import Testing

/// The torture deck (PLAN 0.2): its `morph` state's cue has a motion that enters and one for
/// emphasis.
private let torture = repository.appending(path: "tests/fixtures/torture.scaena")

/// The states, as the timeline plays them, each with its slide: `state/slide`.
private func playing(_ editor: DeckEditor) -> [String] {
    editor.slots.map { "\($0.state)/\($0.slide)" }
}

/// The state list (PLAN 2.35, 3.14), as the browser's strip: a step and a slide added after the
/// state shown, a state renamed, moved, and removed; then the light table (PLAN 2.97): slides
/// moved, copied, and taken out. Each one patch.
@Test func theStateListAndTheLightTableEditAsTheBrowsersDo() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let session = editor.session
    let step = try session.addingState(after: "cover", as: .step)
    #expect(step.id == "cover-2" && step.patch.first?["op"]?.string == "add_state")
    try editor.make(step.patch)
    let slide = try session.addingState(after: "goal", as: .slide)
    #expect(slide.id == "slide")
    try editor.make(slide.patch)
    #expect(
        playing(editor).prefix(6) == [
            "cover/cover", "cover-2/cover", "goal/goal", "goal-why/goal", "goal-bar/goal", "slide/slide",
        ])
    #expect(throws: ScaenaError.self) { try session.addingState(after: "nowhere", as: .step) }

    try editor.make(Restaging.rename("cover-2", to: "opening"))
    try editor.make(Restaging.move("opening", before: true, "cover"))
    #expect(playing(editor).prefix(2) == ["opening/cover", "cover/cover"])
    try editor.make(Restaging.remove("opening"))
    #expect(editor.slots.first?.state == "cover")
    // A name the deck takes for no id: refused, with why.
    #expect(throws: ScaenaError.self) { try editor.make(Restaging.rename("cover", to: "Not An Id")) }

    // The light table: each slide drawn at its last state.
    let slides = Slide.of(editor.slots)
    #expect(slides.prefix(3).map(\.id) == ["cover", "goal", "slide"])
    #expect(slides[1].states == ["goal", "goal-why", "goal-bar"] && slides[1].last == "goal-bar")
    try editor.make(Restaging.moveSlides(["slide"], before: true, "goal"))
    #expect(Slide.of(editor.slots).prefix(3).map(\.id) == ["cover", "slide", "goal"])
    try editor.make(Restaging.duplicateSlides(["goal"]))
    #expect(Slide.of(editor.slots).prefix(4).map(\.id) == ["cover", "slide", "goal", "goal-2"])
    try editor.make(Restaging.removeSlides(["slide", "goal-2"]))
    #expect(Slide.of(editor.slots).prefix(3).map(\.id) == ["cover", "goal", "belief-1"])

    // Several slides moved together: the first where it goes, each other just after it.
    #expect(Restaging.moveSlides(["a", "b"], before: false, "c") == [
        ["op": "move_slide", "slide": "a", "after": "c"], ["op": "move_slide", "slide": "b", "after": "a"],
    ])
}

/// A rehearsal (PLAN 2.63, 3.14): the time each state is shown kept, back and on again; at the
/// end each state reached holds what it took less its cue, to a tenth of a second, as one patch.
@Test func aRehearsalKeepsWhatEachStateTook() throws {
    let slots = try JSONDecoder().decode(
        [ScaenaSession.Slot].self,
        from: Data(
            #"""
            [{"state": "a", "slide": "a", "start": 0, "span": 400, "hold": 0},
             {"state": "b", "slide": "a", "start": 400, "span": 0, "hold": 2000},
             {"state": "c", "slide": "c", "start": 2400, "span": 400, "hold": 0}]
            """#.utf8))
    let began = ContinuousClock.now
    var rehearsal = Rehearsal(slots: slots, at: began)
    #expect(rehearsal.running && rehearsal.state == "a")
    rehearsal.go(1, at: began + .milliseconds(3420))
    #expect(rehearsal.state == "b")
    rehearsal.go(-1, at: began + .milliseconds(4000))
    rehearsal.go(-1, at: began + .milliseconds(4100))
    #expect(rehearsal.state == "a", "back from the first stays")
    rehearsal.go(1, at: began + .milliseconds(5000))
    #expect(rehearsal.here(at: began + .milliseconds(5500)) == 1080)
    rehearsal.stop(at: began + .milliseconds(7000))
    #expect(!rehearsal.running && rehearsal.total() == 7000)

    #expect(rehearsal.kept.map(\.keeps) == [4000, 2600, nil])
    #expect(rehearsal.holds == [
        ["op": "set_state", "id": "a", "prop": "hold", "value": 4000],
        ["op": "set_state", "id": "b", "prop": "hold", "value": 2600],
    ])

    // Going on from the last state ends it.
    var on = Rehearsal(slots: Array(slots.prefix(1)), at: began)
    on.go(1, at: began + .milliseconds(450))
    #expect(!on.running && on.kept.first?.keeps == 100)
}

/// The cue's bars (PLAN 2.44, 3.14), as the browser's: a motion's start and end dragged, each a
/// `time_motion` to a whole ten of ms, the transition's end a `set_state`, and a preset added for
/// emphasis; then a node that leaves, given an exit in the state it leaves.
@Test func aCuesBarsAreTimedAndItsMotionsAdded() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: torture))
    let session = editor.session
    let bars = try session.cue(state: "morph").bars
    #expect(bars.map(\.id) == ["transition", "mf-arrow enter", "mf-badge emphasis"])
    let badge = try #require(bars.first { $0.node == "mf-badge" })
    #expect(badge.from == 420 && badge.to == 820 && badge.delay == 0 && badge.duration == 400)

    let waits = try #require(badge.timing(.delay, by: 104, in: "morph"))
    #expect(waits.said == "mf-badge's emphasis waits 100 ms")
    try editor.make(waits.patch)
    let waited = try #require(try session.cue(state: "morph").bars.first { $0.node == "mf-badge" })
    let lasts = try #require(waited.timing(.duration, by: 196, in: "morph"))
    #expect(lasts.said == "mf-badge's emphasis lasts 600 ms")
    try editor.make(lasts.patch)
    let into = try #require(bars.first?.timing(.duration, by: 80, in: "morph"))
    #expect(into.said == "the transition into morph lasts 500 ms")
    try editor.make(into.patch)
    #expect(bars.first?.timing(.delay, by: 80, in: "morph")?.said == nil, "the transition's start does not move")
    let timed = try #require(try session.cue(state: "morph").bars.first { $0.node == "mf-badge" })
    #expect(timed.delay == 100 && timed.duration == 600 && timed.from == 600 && timed.to == 1200)
    #expect(timed.timing(.delay, by: 2, in: "morph")?.said == nil, "a time that stays as it is")

    // A preset for emphasis on the title, which enters no state here.
    let offers = try session.motionOffers(state: "morph", node: "mf-title")
    #expect(offers.map(\.id) == ["mf-title emphasis"] && offers.first?.presets.contains("pulse") == true)
    #expect(try session.motionOffers(state: "morph", node: "mf-arrow").map(\.id) == ["mf-arrow emphasis"])
    let states = editor.slots.map(\.state)
    try editor.make(try #require(offers.first?.applying("pulse", in: "morph", states: states)))
    #expect(try session.cue(state: "morph").bars.contains { $0.id == "mf-title emphasis" })

    // B1's goal: the cover's title and subtitle leave it, an exit written in the cover.
    let b1Editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let exits = try b1Editor.session.motionOffers(state: "goal", node: nil)
    #expect(exits.map(\.id) == ["title exit", "subtitle exit"] && exits.first?.label == "title leaves")
    let b1States = b1Editor.slots.map(\.state)
    let exit = try #require(exits.first?.applying("fade", in: "goal", states: b1States))
    #expect(exit.first?["state"]?.string == "cover")
    #expect(exits.first?.applying("fade", in: "cover", states: b1States) == nil, "no state before the first")
    try b1Editor.make(exit)
    #expect(try b1Editor.session.cue(state: "goal").bars.map(\.id) == ["transition", "title exit"])
}

/// A new deck opens on a title slide (PLAN 3.30): its one blank state made a slide in the theme's
/// `title` layout, its words in their slots and its picture's place waiting; a deck with something
/// on it is left as it is.
@Test func aNewDeckOpensOnATitleSlide() throws {
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Untitled"))
    #expect(editor.startOnTitleSlide() == "title")
    #expect(editor.slots.map(\.state) == ["title"])
    #expect(try editor.session.waiting(state: "title").map(\.slot) == ["art"])
    #expect(try editor.session.boxes(state: "title").count >= 3, "the kicker, the title, and the subtitle")
    #expect(editor.startOnTitleSlide() == nil, "a title slide stays as it is")
}

/// An empty slot filled from a press on its words (PLAN 3.30): on a slide started in Bullets, its
/// headline deleted, the header waits in its role, and filling it puts back what the slide was
/// started with, one patch; a slot the layout lacks is refused.
@Test func anEmptySlotIsFilledAsTheSlideWasStarted() throws {
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Placeholders"))
    let session = editor.session
    let started = try session.starting(after: "start", layout: "bullets")
    try editor.make(started.patch)
    try editor.make([["op": "remove_node", "id": "bullets-header"]])
    let header = try session.waiting(state: started.id).first { $0.slot == "header" }
    #expect(header?.typed == true && header?.role == "headline" && header?.words == "What this slide says")
    let filled = try session.filling(state: started.id, slot: "header")
    #expect(filled.id == "bullets-header" && filled.patch.first?["op"]?.string == "add_node")
    try editor.make(filled.patch)
    #expect(try session.waiting(state: started.id).allSatisfy { $0.slot != "header" })
    #expect(throws: ScaenaError.self) { try session.filling(state: started.id, slot: "nowhere") }
}
