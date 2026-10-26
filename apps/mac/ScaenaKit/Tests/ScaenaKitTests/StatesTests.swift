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

/// The slides down the side and the light table read each slide to VoiceOver by its headline
/// (lift-up 10-24b USE-4), as the engine says what heads it: B1's cover by its display line.
@Test func eachSlideReadsByItsHeadline() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let cover = try #require(editor.slots.first)
    #expect(cover.headline == "Scaena")
    #expect(SlideReading.row(number: 1, step: 1, headline: cover.headline) == "Slide 1: Scaena")
    #expect(SlideReading.row(number: 3, step: 2, headline: "What we refuse") == "Slide 3, step 2: What we refuse")
    #expect(SlideReading.row(number: 4, step: 1, headline: nil) == "Slide 4")
    #expect(SlideReading.cell(number: 2, headline: "") == "Slide 2")
    #expect(SlideReading.steps(3) == "3 steps" && SlideReading.steps(1).isEmpty)
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

    // From the slide shown, paused for a call: the pause counts toward neither the slide nor the
    // whole, and only the slides rehearsed keep a time (lift-up 10-25b USE-2).
    var here = Rehearsal(slots: slots, from: 1, at: began)
    #expect(here.state == "b")
    here.pause(at: began + .milliseconds(1000))
    #expect(here.paused && here.here(at: began + .milliseconds(60000)) == 1000)
    here.pause(at: began + .milliseconds(60000))
    #expect(!here.paused)
    here.go(1, at: began + .milliseconds(61500))
    #expect(here.state == "c")
    here.pause(at: began + .milliseconds(62000))
    here.go(1, at: began + .milliseconds(90000))
    #expect(!here.running && !here.paused && here.total() == 3000)
    #expect(here.kept.map(\.keeps) == [nil, 2500, 100])
    #expect(Rehearsal(slots: slots, from: 9).state == "a", "a place past the end begins at the first")
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
    #expect(offers.first?.presets.contains("fade") == false, "Fade enters and leaves, and does not emphasize (PLAN 2.116)")
    #expect(try session.motionOffers(state: "morph", node: "mf-arrow").map(\.id) == ["mf-arrow emphasis"])
    let states = editor.slots.map(\.state)
    try editor.make(try #require(offers.first?.applying("pulse", in: "morph", states: states)))
    #expect(try session.cue(state: "morph").bars.contains { $0.id == "mf-title emphasis" })

    // B1's goal: the cover's title and subtitle leave it, an exit written in the cover.
    let b1Editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let exits = try b1Editor.session.motionOffers(state: "goal", node: nil)
    #expect(exits.map(\.id) == ["title exit", "subtitle exit"] && exits.first?.label == "title leaves")
    #expect(exits.first?.presets.contains("fade") == true && exits.first?.presets.contains("pulse") == false, "Pulse only emphasizes")
    let b1States = b1Editor.slots.map(\.state)
    let exit = try #require(exits.first?.applying("fade", in: "goal", states: b1States))
    #expect(exit.first?["state"]?.string == "cover")
    #expect(exits.first?.applying("fade", in: "cover", states: b1States) == nil, "no state before the first")
    try b1Editor.make(exit)
    #expect(try b1Editor.session.cue(state: "goal").bars.map(\.id) == ["transition", "title exit"])

    // A push into the goal moves the cover whole (SPEC §3.9): its exits play none, and none is offered.
    let push: JSONValue = ["op": "set_state", "id": "goal", "prop": "transition/style", "value": "push"]
    try b1Editor.make([push])
    #expect(try b1Editor.session.cue(state: "goal").bars.map(\.id) == ["transition"])
    #expect(try b1Editor.session.motionOffers(state: "goal", node: nil).isEmpty, "no exit under a slide transition")
}

/// The Animate tab offers every slide transition, the five that show the slide entered through
/// shapes that grow among them (PLAN 2.149), each in a person's words, and one chosen is the cue's.
@Test func theStateOffersTheTransitionsThatGrowInPlainWords() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let fields = try editor.session.stateChoices(state: "goal").fields
    let style = try #require(fields.first { $0.prop == "transition/style" })
    let growing = ["circle", "circleWave", "diamond", "diamondWave", "radial"]
    guard case .word(let words) = style.takes else {
        Issue.record("a transition's style is one of its words: \(style.takes)")
        return
    }
    #expect(words == ["dissolve", "push", "cover", "reveal", "wipe"] + growing)
    #expect(growing.map { style.said[$0] } == ["Circle", "Circle Wave", "Diamond", "Diamond Wave", "Radial"])

    for chosen in growing {
        try editor.make([["op": "set_state", "id": "goal", "prop": "transition/style", "value": .string(chosen)]])
        let cue = try editor.session.cue(state: "goal")
        #expect(cue.transition.style == chosen && cue.transition.duration > 0, "\(chosen) plays")
        #expect(cue.bars.map(\.id) == ["transition"], "\(chosen) moves the slide left whole: no exit plays")
    }
}

/// The canvas and the cue select together, as the browser's: the bars of what the canvas selects
/// stand out, and those of what a group holds; a node with no motion here, none.
@Test func theBarsOfWhatTheCanvasSelectsStandOut() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: torture))
    let session = editor.session
    let boxes = try session.boxes(state: "morph")
    let cue = try session.cue(state: "morph")
    #expect(cue.chosen(["mf-badge"], boxes: boxes) == ["mf-badge emphasis"])
    #expect(cue.chosen(["mf-arrow", "mf-badge"], boxes: boxes) == ["mf-arrow enter", "mf-badge emphasis"])
    #expect(cue.chosen(["mf-title"], boxes: boxes).isEmpty, "no motion of its own here")
    #expect(cue.chosen([], boxes: boxes).isEmpty)

    // A member of a group given an emphasis: the group selected, the member's bar stands out.
    let offers = try session.motionOffers(state: "morph", node: "mf-pair-a")
    let emphasis = try #require(offers.first { $0.motion == "emphasis" })
    try editor.make(try #require(emphasis.applying("pulse", in: "morph", states: editor.slots.map(\.state))))
    let grouped = try session.cue(state: "morph")
    let held = try session.boxes(state: "morph")
    #expect(grouped.chosen(["mf-pair"], boxes: held) == ["mf-pair-a emphasis"])
    #expect(grouped.chosen(["mf-pair-b"], boxes: held).isEmpty, "a member does not hold its sibling")
    #expect(grouped.bars.first { $0.node == "mf-pair-a" }?.split == nil)
}

/// A new deck opens on a title slide (PLAN 3.30): its one blank state made a slide in the theme's
/// `title` layout, its words in their slots and the theme's backdrop in its picture's place, as the
/// theme chooser draws it, so no place waits (W304); a deck with something on it is left as it is.
@Test func aNewDeckOpensOnATitleSlide() throws {
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Untitled"))
    #expect(editor.startOnTitleSlide() == "title")
    #expect(editor.slots.map(\.state) == ["title"])
    #expect(try editor.session.waiting(state: "title").isEmpty, "the backdrop stands in the picture's place")
    #expect(try editor.session.boxes(state: "title").count >= 4, "the backdrop, the kicker, the title, and the subtitle")
    #expect(editor.startOnTitleSlide() == nil, "a title slide stays as it is")
}

/// An empty slot's words as the canvas may stand them (PLAN 3.30): a title slide's picture's
/// place, as Add Slide starts it, a slot drawn tall and narrow, on a line, then two, and so on
/// down to its longest word's width, each line whole words; the canvas shows the first way that fits, so no word is cut, as
/// "A pictur…" was in the slot drawn 92 points wide. Each way holds every word, in order, on a line
/// more than the way before it.
@Test func anEmptySlotsWordsBreakOnlyBetweenWords() throws {
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Untitled"))
    let started = try editor.session.starting(after: "start", layout: "title")
    try editor.make(started.patch)
    let art = try #require(try editor.session.waiting(state: started.id).first { $0.slot == "art" })
    #expect(art.words == "A picture, or a backdrop")
    #expect(
        art.lines == [
            ["A picture, or a backdrop"], ["A picture, or", "a backdrop"], ["A picture,", "or a", "backdrop"],
            ["A", "picture,", "or a", "backdrop"],
        ])
    for words in [art.words, "What it's about, in a line", "Your name · the date", "Overview", " two  spaces "] {
        let ways = WaitingSlot.lines(words)
        let all = words.split(whereSeparator: \.isWhitespace).map(String.init)
        for way in ways {
            let said = way.flatMap { $0.split(separator: " ") }.map(String.init)
            #expect(said == all, "\(words): every word, whole and in order, in \(way)")
        }
        #expect(ways.first?.count == 1, "\(words): on a line first")
        #expect(zip(ways, ways.dropFirst()).allSatisfy { $0.count < $1.count }, "\(words): a line more each way")
        #expect(ways.last?.map(\.count).max() == all.map(\.count).max(), "\(words): last, its longest word wide")
    }
    #expect(WaitingSlot.lines(" ").isEmpty)
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
    // Its words still read as the slot's prompt: typing begins with them selected whole.
    #expect(try session.prompted(state: started.id, node: "bullets-header"))
    #expect(try !session.prompted(state: started.id, node: "nowhere"))
    #expect(throws: ScaenaError.self) { try session.filling(state: started.id, slot: "nowhere") }
}

/// The timeline's ruler (PLAN 3.14), as an animation pane's scale of seconds: marks a round step
/// apart, from the cue's start to the lanes' end, never closer than there is room for each one's
/// time.
@Test func theTimelinesRulerMarksTheSeconds() {
    #expect(Cue.ticks(across: 2000, width: 300) == [0, 500, 1000, 1500, 2000])
    #expect(Cue.ticks(across: 2100, width: 150) == [0, 1000, 2000])
    #expect(Cue.ticks(across: 1000, width: 600) == [0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000])
    #expect(Cue.ticks(across: 90_000, width: 100) == [0, 60_000])
    #expect(Cue.ticks(across: 0, width: 300).isEmpty)
    for (scale, width) in [(1300.0, 250.0), (5000.0, 480.0), (12_500.0, 200.0)] {
        let marks = Cue.ticks(across: scale, width: width)
        #expect(marks.first == 0 && (marks.last ?? .infinity) <= scale)
        #expect(zip(marks, marks.dropFirst()).allSatisfy { ($1 - $0) / scale * width >= 47.99 })
    }
}

/// A rehearsal carries each state's notes as a presenter reads them (SPEC §2.6, lift-up USE-3): the
/// state's own, else its beat's, which the stage shows beside the slide with the next slide's name,
/// and steps with the rehearsal.
@Test func aRehearsalCarriesEachStatesNotes() throws {
    let session = try ScaenaSession(files: example("revenue"))
    let slots = try session.timeline()
    var rehearsal = try Rehearsal(slots: slots, notes: slots.map { try session.notes(state: $0.state) })
    #expect(rehearsal.notes.count == slots.count)
    #expect(rehearsal.current?.text == "Open on the title. Don't talk over the build.")
    #expect(rehearsal.next?.state == "revenue")
    rehearsal.go(1)
    #expect(rehearsal.current?.text.hasPrefix("Pause on the Q3 bar before the build.") == true, "its beat's")
    // Given none, it shows none.
    #expect(Rehearsal(slots: slots).current == nil)
}
