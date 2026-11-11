import XCTest

/// Play on the iPad alone (PLAN 4.8), on B1, its 40 states: the stage fills the iPad from the slide
/// shown and reads as that slide does; a swipe to the left goes on and one to the right goes back,
/// a tap goes on, a touch held is the laser and turns no slide (PLAN 2.105), and a pinch closed
/// ends the show, the window showing the slide it stopped on. With an external display
/// or an AirPlay screen the stage goes there and the iPad shows the presenter's view; a simulator
/// driven by XCUITest attaches neither.
final class PresentingTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testPlayFillsTheIPad() throws {
        let app = launched()
        try openB1(in: app)
        let play = app.buttons.matching(NSPredicate(format: "label == 'Play'")).firstMatch
        XCTAssertTrue(play.waitForExistence(timeout: 10), "no Play: \(app.debugDescription)")
        play.tap()
        let stage = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'stage'")).firstMatch
        guard stage.waitForExistence(timeout: 10) else {
            keep(app, as: "not-played")
            throw Unseen(description: "Play showed no stage: \(app.debugDescription)")
        }
        let shown = { stage.value as? String ?? "" }
        XCTAssertEqual(shown(), "1 of 40", "the stage is not on the cover")
        XCTAssertTrue(stage.label.contains("Scaena"), "the stage reads as no cover: \(stage.label)")
        settle()
        keep(app, as: "played")

        XCTContext.runActivity(named: "A swipe to the left goes on, and one to the right goes back") { _ in
            stage.swipeLeft()
            XCTAssertTrue(wait(5) { shown() == "2 of 40" }, "the swipe went to \(shown())")
            // Its cue played out, a swipe back shows the cover at rest.
            settle()
            keep(app, as: "swiped")
            stage.swipeRight()
            XCTAssertTrue(wait(5) { shown() == "1 of 40" }, "the swipe back went to \(shown())")
        }

        XCTContext.runActivity(named: "A tap goes on") { _ in
            stage.tap()
            XCTAssertTrue(wait(5) { shown() == "2 of 40" }, "the tap went to \(shown())")
            settle()
        }

        XCTContext.runActivity(named: "A touch held is the laser, and turns no slide") { _ in
            // Held, then dragged across the slide as a swipe would go: the laser follows the finger
            // (PLAN 2.105), and the slide stays.
            let from = stage.coordinate(withNormalizedOffset: CGVector(dx: 0.7, dy: 0.5))
            let to = stage.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5))
            from.press(forDuration: 1.0, thenDragTo: to)
            settle()
            XCTAssertEqual(shown(), "2 of 40", "the touch held turned the slide")
            // A quick swipe after it still turns it.
            stage.swipeLeft()
            XCTAssertTrue(wait(5) { shown() == "3 of 40" }, "a swipe after the laser went to \(shown())")
            settle()
        }

        XCTContext.runActivity(named: "A pinch closed ends the show") { _ in
            stage.pinch(withScale: 0.4, velocity: -1)
            XCTAssertTrue(wait(10) { !stage.exists }, "the stage stayed: \(app.debugDescription)")
            let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
            XCTAssertTrue(canvas.waitForExistence(timeout: 10), "the window is not back: \(app.debugDescription)")
            // The show stopped on slide 2, as a presentation app leaves you where you stopped.
            XCTAssertTrue(wait(10) { canvas.label.hasPrefix("Slide 2") }, "the window shows \(canvas.label), not slide 2")
            keep(app, as: "ended")
        }
    }

    /// ⌥⌘K opens the Play menu's Remote… (lift-up 20261110), as a keyboard reaches it on the iPad,
    /// whose menu bar a test does not: let a remote play the deck, and it shows the code a remote
    /// joins with, eight digits (in two groups of four on the sheet, `Remote.shown`, which
    /// ScaenaKit's `RemoteTests` holds; the code's label reads them one by one); let it not, and
    /// the code goes.
    @MainActor
    func testRemoteShowsTheCodeARemoteJoinsWith() throws {
        let app = launched()
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        settle()
        // A tap where nothing draws gives the canvas the keyboard, which hands ⌥⌘K to the menu bar.
        canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
        app.typeKey("k", modifierFlags: [.command, .option])
        let on = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'remote-on'")).firstMatch
        guard on.waitForExistence(timeout: 10) else {
            keep(app, as: "remote-not-opened")
            throw Unseen(description: "⌥⌘K opened no Remote…: \(app.debugDescription)")
        }
        let isOn = { (on.value as? String) == "1" }
        XCTAssertFalse(isOn(), "Remote… opened let on")
        // The switch stands at the row's trailing end, its label before it.
        let flip = { on.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }
        flip()
        let code = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'remote-code'")).firstMatch
        guard code.waitForExistence(timeout: 10) else {
            keep(app, as: "remote-no-code")
            throw Unseen(description: "let on, Remote… shows no code: \(app.debugDescription)")
        }
        keep(app, as: "remote-code")
        XCTAssertTrue(isOn(), "the switch reads off with the code shown")
        let digits = code.label.hasPrefix("Code ") ? code.label.dropFirst(5).split(separator: " ") : []
        XCTAssertEqual(digits.count, 8, "the code reads \(code.label)")
        XCTAssertTrue(digits.allSatisfy { $0.count == 1 && $0.allSatisfy(\.isNumber) }, "the code reads \(code.label)")
        flip()
        XCTAssertTrue(wait(5) { !code.exists }, "let off, Remote… still shows a code: \(code.label)")
        app.buttons.matching(NSPredicate(format: "label == 'Done'")).firstMatch.tap()
        XCTAssertTrue(wait(5) { !on.exists }, "Done left Remote… up")
    }
}
