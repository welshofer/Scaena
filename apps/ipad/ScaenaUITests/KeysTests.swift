import XCTest

/// The canvas by keys alone (PLAN 4.6), on B1's cover with a hardware keyboard, as the Mac's canvas
/// takes them (PLAN 3.17), each key the patch its gesture makes: a tap where nothing draws gives the
/// canvas the keyboard; Tab selects in reading order and Shift+Tab goes back; the arrows nudge the
/// title as its drag would, and ⌘Z, Undo by its key, takes each nudge back, the keys still the
/// canvas's; Return types in the title and Escape stops, the keys the canvas's again; ⌘D,
/// Duplicate by its key, copies it; and Escape selects what holds it. Each node is an element that
/// reads as the reader hears it.
final class KeysTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testTheCanvasByKeysAlone() throws {
        let app = launched()
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        // The cover reads as the reader hears it: its title, then its subtitle.
        let titles = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'"))
        let title = titles.firstMatch
        let subtitle = canvas.descendants(matching: .any)
            .matching(NSPredicate(format: "label BEGINSWITH 'Why this exists'")).firstMatch
        guard title.waitForExistence(timeout: 10), subtitle.exists else {
            throw Unseen(description: "the cover reads no title and subtitle: \(app.debugDescription)")
        }
        settle()

        try XCTContext.runActivity(named: "Tab selects in reading order, and Shift+Tab goes back") { _ in
            // A tap where nothing draws: nothing selected, and the canvas has the keyboard.
            canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            guard wait(5, { title.isSelected }) else {
                keep(app, as: "tab-selected-nothing")
                throw Unseen(description: "Tab selected no title: \(app.debugDescription)")
            }
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            XCTAssertTrue(wait(5) { subtitle.isSelected && !title.isSelected }, "Tab did not go on to the subtitle")
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: .shift)
            XCTAssertTrue(wait(5) { title.isSelected && !subtitle.isSelected }, "Shift+Tab did not go back to the title")
            keep(app, as: "tabbed")
        }

        try XCTContext.runActivity(named: "The arrows nudge the title as its drag would, and ⌘Z takes each nudge back") { _ in
            // A canvas unit a press, about half a point at this size: eight of them, each a step to undo.
            let before = title.frame
            for _ in 0..<8 {
                app.typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: [])
            }
            guard wait(5, { title.frame.minX > before.minX + 2 }) else {
                throw Unseen(description: "the arrows nudged nothing: \(before) → \(title.frame)")
            }
            keep(app, as: "stepped")
            for _ in 0..<8 {
                app.typeKey("z", modifierFlags: .command)
            }
            XCTAssertTrue(wait(5) { abs(title.frame.minX - before.minX) < 0.5 }, "⌘Z left a nudge: \(title.frame)")
        }

        try XCTContext.runActivity(named: "After ⌘Z the keys are still the canvas's: Escape, then Tab") { _ in
            // Escape selects what holds the title, the slide, as a key with no modifier the canvas
            // reads; Tab, which the canvas declines with ⌘, selects the title again.
            app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
            guard wait(5, { !title.isSelected }) else {
                keep(app, as: "undone-escape")
                throw Unseen(description: "after ⌘Z, Escape did not reach the canvas: \(app.debugDescription)")
            }
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            guard wait(5, { title.isSelected }) else {
                keep(app, as: "undone-tab")
                throw Unseen(
                    description: "after ⌘Z, Escape reached the canvas and Tab did not: \(app.debugDescription)")
            }
        }

        try XCTContext.runActivity(named: "Return types in the title; Escape stops, and the keys are the canvas's") { _ in
            app.typeKey(XCUIKeyboardKey.return, modifierFlags: [])
            let typing = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'typing'")).firstMatch
            guard typing.waitForExistence(timeout: 5) else {
                keep(app, as: "return-typed-nothing")
                // Which key the canvas did not hear: the keypad's Enter is Return to it too.
                app.typeKey(XCUIKeyboardKey.enter, modifierFlags: [])
                let entered = typing.waitForExistence(timeout: 5)
                throw Unseen(
                    description: "Return typed in nothing, though Escape and Tab reached the canvas; Enter "
                        + "\(entered ? "did" : "did not either"): \(app.debugDescription)")
            }
            app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
            XCTAssertTrue(wait(5) { !typing.exists && title.isSelected }, "Escape did not stop typing")
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            XCTAssertTrue(wait(5) { subtitle.isSelected }, "the canvas did not take the keys back")
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: .shift)
            XCTAssertTrue(wait(5) { title.isSelected }, "Shift+Tab did not go back to the title")
        }

        try XCTContext.runActivity(named: "⌘D duplicates the title, and ⌘Z takes the copy back") { _ in
            app.typeKey("d", modifierFlags: .command)
            guard wait(5, { titles.count == 2 }) else {
                throw Unseen(description: "⌘D made no copy: \(app.debugDescription)")
            }
            settle()
            keep(app, as: "duplicated")
            // B1 is left as it was: the tests after this one open it too.
            app.typeKey("z", modifierFlags: .command)
            XCTAssertTrue(wait(5) { titles.count == 1 }, "⌘Z left the copy")
        }

        XCTContext.runActivity(named: "Escape selects what holds the title: the slide") { _ in
            // A tap selects it, and gives the canvas the keyboard.
            title.tap()
            XCTAssertTrue(wait(5) { title.isSelected }, "the title is not selected")
            app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
            XCTAssertTrue(wait(5) { !title.isSelected && !subtitle.isSelected }, "Escape left a node selected")
        }
    }
}
