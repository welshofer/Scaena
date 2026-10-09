import XCTest

/// Play on the iPad alone (PLAN 4.8), on B1, its 40 states: the stage fills the iPad from the slide
/// shown and reads as that slide does; a swipe to the left goes on and one to the right goes back,
/// a tap goes on, and a pinch closed ends the show, the window as it was. With an external display
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

        XCTContext.runActivity(named: "A pinch closed ends the show") { _ in
            stage.pinch(withScale: 0.4, velocity: -1)
            XCTAssertTrue(wait(10) { !stage.exists }, "the stage stayed: \(app.debugDescription)")
            let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
            XCTAssertTrue(canvas.waitForExistence(timeout: 10), "the window is not back: \(app.debugDescription)")
            keep(app, as: "ended")
        }
    }
}
