import XCTest

/// The canvas by touch (PLAN 4.3), on B1's cover, a gesture at a time and in order, each the
/// patch the Mac's pointer makes for it: a tap selects the title, a pinch zooms and another takes
/// the whole canvas back, a double tap types in it and the keys type there (PLAN 4.4), a drag
/// moves it, and a long press offers what is done to it, whose Duplicate copies it; then, in the
/// light table, a slide dragged onto another moves after it.
final class TouchTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testTheCanvasByTouch() throws {
        let app = launched()
        try openB1(in: app)
        // The canvas reads as the slide it shows; a test finds it by its id.
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        XCTAssertEqual(canvas.label, "Slide 1", "the canvas reads as no slide")
        // The title reads "Scaena", and once typed in, more.
        let titles = canvas.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'Scaena'"))
        let title = titles.firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 10), "no title on the canvas: \(app.debugDescription)")
        settle()

        XCTContext.runActivity(named: "A tap selects the title") { _ in
            title.tap()
            XCTAssertTrue(wait(5) { title.isSelected }, "the title is not selected: \(app.debugDescription)")
        }

        try XCTContext.runActivity(named: "A pinch zooms, and another shows the whole canvas") { _ in
            let before = title.frame
            canvas.pinch(withScale: 2, velocity: 1)
            guard wait(5, { shown(canvas) != "whole" }) else {
                throw Unseen(description: "the pinch zoomed nothing: \(before) → \(title.frame), \(shown(canvas))")
            }
            keep(app, as: "pinched")
            // Two fingers are no long press: nothing is offered.
            let offered = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Duplicate'")).firstMatch
            XCTAssertFalse(offered.exists, "a pinch offered a menu: \(app.debugDescription)")
            let closer = shown(canvas)
            canvas.pinch(withScale: 0.4, velocity: -1)
            let whole = wait(5) { shown(canvas) == "whole" && abs(title.frame.width - before.width) < 2 }
            keep(app, as: "unpinched")
            guard whole else {
                throw Unseen(
                    description:
                        "the canvas is not whole again: \(before) → \(title.frame), \(closer) → \(shown(canvas)), the canvas at \(canvas.frame)"
                )
            }
        }

        try XCTContext.runActivity(named: "A double tap types in the title, and the keys type there") { _ in
            title.doubleTap()
            let typing = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'typing'")).firstMatch
            guard typing.waitForExistence(timeout: 5), typing.label == "Editing text" else {
                throw Unseen(description: "a double tap typed in nothing: \(app.debugDescription)")
            }
            // The keyboard's keys, through `UITextInput`, at the caret the double tap put there.
            app.typeText("!")
            let typed = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena!'")).firstMatch
            guard typed.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the keys typed nothing in the title: \(app.debugDescription)")
            }
            keep(app, as: "typing")
            // A tap where nothing draws stops typing.
            canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
            XCTAssertTrue(wait(5) { !typing.exists }, "still typing: \(app.debugDescription)")
        }

        try XCTContext.runActivity(named: "A drag moves the title") { _ in
            title.tap()
            let before = title.frame
            let from = title.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            from.press(forDuration: 0.1, thenDragTo: from.withOffset(CGVector(dx: 0, dy: 160)))
            guard wait(5, { abs(title.frame.midY - before.midY) > 20 }) else {
                throw Unseen(description: "the drag moved nothing: \(before) → \(title.frame)")
            }
            keep(app, as: "dragged")
        }

        try XCTContext.runActivity(named: "A long press offers what is done to the title, and Duplicate copies it") { _ in
            title.press(forDuration: 1.2)
            let duplicate = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Duplicate'")).firstMatch
            guard duplicate.waitForExistence(timeout: 5) else {
                throw Unseen(description: "a long press offered nothing: \(app.debugDescription)")
            }
            keep(app, as: "offered")
            XCTAssertTrue(title.isSelected, "the title pressed is not selected")
            duplicate.tap()
            guard wait(5, { titles.count == 2 }) else {
                throw Unseen(description: "Duplicate made no copy: \(app.debugDescription)")
            }
            settle()
            keep(app, as: "duplicated")
        }

        try XCTContext.runActivity(named: "In the light table, a slide dragged onto another moves after it") { _ in
            // The toolbar's View menu, as a presentation app's, shows the light table.
            let view = app.buttons.matching(NSPredicate(format: "label == 'View'")).firstMatch
            guard view.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no View in the toolbar: \(app.debugDescription)")
            }
            view.tap()
            let table = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Light Table'")).firstMatch
            guard table.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the View menu offers no light table: \(app.debugDescription)")
            }
            table.tap()
            // Each slide reads its number and its steps, and a test finds it by its id: B1's first
            // is the cover, its third belief-1.
            let cells = app.descendants(matching: .any)
            let cover = cells.matching(NSPredicate(format: "identifier == 'slide-cover'")).firstMatch
            let third = cells.matching(NSPredicate(format: "identifier == 'slide-belief-1'")).firstMatch
            guard cover.waitForExistence(timeout: 10), third.exists, cover.label.hasPrefix("Slide 1"),
                third.label.hasPrefix("Slide 3")
            else {
                throw Unseen(description: "the light table shows no cover and belief-1: \(app.debugDescription)")
            }
            keep(app, as: "slides")
            // A slide lifts once held, as any drag on the iPad does, and lands where it is let go.
            cover.press(forDuration: 1.5, thenDragTo: third, withVelocity: .slow, thenHoldForDuration: 1)
            let moved = cells.matching(NSPredicate(format: "identifier == 'slide-cover' AND label BEGINSWITH 'Slide 3'")).firstMatch
            guard moved.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the cover did not move after belief-1: \(app.debugDescription)")
            }
            keep(app, as: "slide-moved")
        }
    }

    /// How close the canvas is shown, as it tells VoiceOver: "whole" where nothing is zoomed.
    @MainActor
    private func shown(_ canvas: XCUIElement) -> String {
        let value = canvas.value as? String ?? ""
        return value.isEmpty ? "whole" : value
    }
}
