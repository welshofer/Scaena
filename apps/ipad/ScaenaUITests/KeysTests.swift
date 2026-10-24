import XCTest

/// The canvas by keys alone (PLAN 4.6), on B1's cover with a hardware keyboard, as the Mac's canvas
/// takes them (PLAN 3.17), each key the patch its gesture makes: a tap where nothing draws gives the
/// canvas the keyboard; Tab selects in reading order and Shift+Tab goes back; the arrows nudge the
/// title as its drag would, and ⌘Z, Undo by its key, takes each nudge back, the keys still the
/// canvas's; Return types in the title and Escape stops, the keys the canvas's again; ⌘D,
/// Duplicate by its key, copies it; and Escape selects what holds it. Each node is an element that
/// reads as the reader hears it. Return is sent as a newline, the Return key of the simulator's
/// keyboard. Where Escape is not heard, ⌘., the system's cancel key, which the canvas takes as
/// Escape, is pressed in its place, and that is said beside the results. A key the canvas does not
/// hear is said, and the test goes on as far as it can without it and fails at its end, saying
/// every such key; where neither Escape nor ⌘. is heard, Escape is tried in the find bar's text
/// field, which says whether it reaches the app at all.
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

        // Each key the canvas did not hear, said as the test goes on, so that one run says it of
        // every key; the test fails at its end where any is said. Where Escape is not heard and ⌘.,
        // which the canvas takes as Escape, is, that is said beside the results instead.
        var unheard: [String] = []
        var cancelled: [String] = []

        try XCTContext.runActivity(named: "Escape selects what holds the title, before any edit; Tab selects it again") { _ in
            switch cancel(app, until: { !title.isSelected }) {
            case .escape:
                break
            case .commandPeriod:
                cancelled.append("before any edit")
            case .neither:
                keep(app, as: "escape-unheard")
                unheard.append("neither Escape nor ⌘. reached the canvas before any edit; \(escapeInTheFindBar(app))")
                // The canvas given the keyboard again, the title selected, for what follows.
                title.tap()
                guard wait(5, { title.isSelected }) else {
                    throw Unseen(description: "a tap did not select the title again: \(app.debugDescription)")
                }
                return
            }
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            XCTAssertTrue(wait(5) { title.isSelected }, "Tab did not select the title again")
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
            // reads; Tab, which the canvas declines with ⌘, selects the title again. Where neither
            // Escape nor ⌘. is heard, Tab goes on to the subtitle: the keys are still the canvas's.
            let heard = cancel(app, until: { !title.isSelected })
            switch heard {
            case .escape: break
            case .commandPeriod: cancelled.append("after ⌘Z")
            case .neither:
                keep(app, as: "undone-escape")
                unheard.append("after ⌘Z, neither Escape nor ⌘. reached the canvas")
            }
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            guard wait(5, { heard == .neither ? subtitle.isSelected : title.isSelected }) else {
                keep(app, as: "undone-tab")
                throw Unseen(description: "after ⌘Z, Tab did not reach the canvas: \(app.debugDescription)")
            }
            if heard == .neither {
                app.typeKey(XCUIKeyboardKey.tab, modifierFlags: .shift)
                guard wait(5, { title.isSelected }) else {
                    throw Unseen(description: "Shift+Tab did not go back to the title: \(app.debugDescription)")
                }
            }
        }

        try XCTContext.runActivity(named: "Return types in the title; Escape stops, and the keys are the canvas's") { _ in
            let typing = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'typing'")).firstMatch
            // Return as the simulator's keyboard sends it: XCUITest's `.return` sends it no key
            // there, and its `.enter` sends Ctrl+C; a newline is the Return key.
            app.typeKey("\n", modifierFlags: [])
            guard typing.waitForExistence(timeout: 5) else {
                keep(app, as: "return-typed-nothing")
                unheard.append("Return typed in nothing")
                return
            }
            switch cancel(app, until: { !typing.exists && title.isSelected }) {
            case .escape: break
            case .commandPeriod: cancelled.append("while typing")
            case .neither:
                keep(app, as: "typing-not-stopped")
                unheard.append("neither Escape nor ⌘. stopped typing")
                // Typing stopped as a tap stops it, where nothing draws, and the title selected again.
                canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
                title.tap()
                guard wait(5, { !typing.exists && title.isSelected }) else {
                    throw Unseen(description: "typing did not stop: \(app.debugDescription)")
                }
            }
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
            switch cancel(app, until: { !title.isSelected && !subtitle.isSelected }) {
            case .escape: break
            case .commandPeriod: cancelled.append("the title selected by a tap")
            case .neither: unheard.append("neither Escape nor ⌘., the title selected by a tap, selected the slide")
            }
        }

        if !cancelled.isEmpty {
            XCTContext.runActivity(
                named: "Escape reached nothing in the app; ⌘., which the canvas takes as Escape, did: "
                    + cancelled.joined(separator: "; ")
            ) { _ in }
        }
        if !unheard.isEmpty {
            throw Unseen(description: "keys the canvas did not hear: " + unheard.joined(separator: "; "))
        }
    }

    /// Zoomed in, the arrows pan, and PageDown and PageUp turn the slides (PLAN 4.12, as the
    /// browser's, PLAN 2.125, 2.126), on B1's cover with a hardware keyboard. The toolbar's Zoom In
    /// brings the slide a step closer, about its middle, and a tap where nothing draws gives the
    /// canvas the keyboard with nothing selected. Then → pans what shows a tenth of itself, the
    /// subtitle's end moving a tenth of the canvas's width to the left; ← pans it back; and Shift+→
    /// pans farther. None selects anything, and the canvas still reads as the cover. PageDown shows
    /// the second slide, the desk under it saying so, PageUp the first again, and PageUp there says
    /// it is the first. A key the canvas does not hear is said, and the test fails at its end saying
    /// every such key. The simulator's keyboard sends no PageDown or PageUp, so there the test keeps
    /// a picture of the canvas and goes on; on an iPad, a PageDown unheard fails it.
    @MainActor
    func testTheKeysPanWhatIsZoomedInAndTurnTheSlides() throws {
        let app = launched()
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        let title = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'")).firstMatch
        let subtitle = canvas.descendants(matching: .any)
            .matching(NSPredicate(format: "label BEGINSWITH 'Why this exists'")).firstMatch
        guard title.waitForExistence(timeout: 10), subtitle.exists else {
            throw Unseen(description: "the cover reads no title and subtitle: \(app.debugDescription)")
        }
        // What the canvas says, on the desk under the slide.
        let hint = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'hint'")).firstMatch
        let said = { hint.exists ? hint.label : "nothing" }
        settle()
        var unheard: [String] = []

        try XCTContext.runActivity(named: "The toolbar's Zoom In brings the slide a step closer") { _ in
            let zoom = app.buttons.matching(NSPredicate(format: "identifier == 'zoom'")).firstMatch
            guard zoom.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no Zoom in the toolbar: \(app.debugDescription)")
            }
            zoom.tap()
            let closer = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Zoom In'")).firstMatch
            guard closer.waitForExistence(timeout: 5) else {
                keep(app, as: "zoom-menu")
                throw Unseen(description: "the Zoom menu offers no Zoom In: \(app.debugDescription)")
            }
            closer.tap()
            guard wait(5, { !(canvas.value as? String ?? "").isEmpty }) else {
                throw Unseen(description: "Zoom In zoomed nothing: \(app.debugDescription)")
            }
            // A tap where nothing draws, past the subtitle's end and under it: nothing selected,
            // and the canvas has the keyboard.
            canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.97)).tap()
            XCTAssertTrue(wait(5) { !title.isSelected && !subtitle.isSelected }, "the tap selected something")
            settle(1)
            keep(app, as: "zoomed-in")
        }

        try XCTContext.runActivity(named: "With nothing selected, → pans a tenth of what shows, ← back, and Shift+→ farther") { _ in
            let (was, wide) = (subtitle.frame, canvas.frame.width)
            app.typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: [])
            guard wait(5, { abs(subtitle.frame.maxX - was.maxX) > 1 }) else {
                keep(app, as: "arrow-panned-nothing")
                throw Unseen(description: "→ panned nothing: the subtitle at \(was), then \(subtitle.frame)")
            }
            settle(1)
            let moved = was.maxX - subtitle.frame.maxX
            XCTAssertEqual(
                moved, wide / 10, accuracy: 2, "→ moved the subtitle \(moved) points, not a tenth of the \(wide) shown")
            XCTAssertFalse(title.isSelected || subtitle.isSelected, "→ selected something")
            XCTAssertEqual(canvas.label, "Slide 1", "→ showed another slide")
            keep(app, as: "panned")
            app.typeKey(XCUIKeyboardKey.leftArrow, modifierFlags: [])
            XCTAssertTrue(
                wait(5) { abs(subtitle.frame.maxX - was.maxX) < 1 }, "← did not pan back: \(was) → \(subtitle.frame)")
            // The simulator's keyboard can lose the Shift of a press, which the app then hears as a
            // plain →: its key log has the arrow with no flags, and no Shift before it. Only where
            // the subtitle panned just a tenth, as → pans it, is it panned back and Shift+→ pressed
            // again.
            let farther = { was.maxX - subtitle.frame.maxX > moved + 2 }
            app.typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: .shift)
            if !wait(5, farther), abs(was.maxX - subtitle.frame.maxX - moved) < 1 {
                keep(app, as: "shift-unheard")
                app.typeKey(XCUIKeyboardKey.leftArrow, modifierFlags: [])
                XCTAssertTrue(
                    wait(5) { abs(subtitle.frame.maxX - was.maxX) < 1 }, "← did not pan back: \(was) → \(subtitle.frame)")
                app.typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: .shift)
            }
            XCTAssertTrue(wait(5, farther), "Shift+→ panned no farther than →: \(was) → \(subtitle.frame)")
            XCTAssertFalse(title.isSelected || subtitle.isSelected, "Shift+→ selected something")
            keep(app, as: "panned-farther")
        }

        XCTContext.runActivity(named: "PageDown shows the next slide and PageUp the one before, the desk saying which") { _ in
            app.typeKey(XCUIKeyboardKey.pageDown, modifierFlags: [])
            guard wait(5, { canvas.label == "Slide 2" }) else {
                #if targetEnvironment(simulator)
                // The simulator's keyboard sends no PageDown: the app's key log hears each arrow before
                // it and no PageDown at all. An iPad's keyboard is the reading (docs/gate-4.md), and
                // ScaenaKit's `pageDownAndPageUpAreTheIPadCanvassOwnKeyCommands` holds the commands.
                keep(app, as: "page-down-unsent")
                #else
                keep(app, as: "page-down-unheard")
                unheard.append("PageDown showed no next slide: the canvas reads \(canvas.label)")
                #endif
                return
            }
            XCTAssertTrue(wait(5) { said() == "Slide 2 shown" }, "the desk says \(said()) of PageDown")
            keep(app, as: "paged-down")
            app.typeKey(XCUIKeyboardKey.pageUp, modifierFlags: [])
            guard wait(5, { canvas.label == "Slide 1" }) else {
                unheard.append("PageUp showed no slide before: the canvas reads \(canvas.label)")
                return
            }
            XCTAssertTrue(wait(5) { said() == "Slide 1 shown" }, "the desk says \(said()) of PageUp")
            app.typeKey(XCUIKeyboardKey.pageUp, modifierFlags: [])
            XCTAssertTrue(wait(5) { said() == "This is the first slide" }, "PageUp on the first slide says \(said())")
            XCTAssertEqual(canvas.label, "Slide 1", "PageUp went before the first slide")
        }

        if !unheard.isEmpty {
            throw Unseen(description: "keys the canvas did not hear: " + unheard.joined(separator: "; "))
        }
    }

    /// What was heard of the canvas's Escape (PLAN 4.6).
    /// Tab in code (PLAN 2.139), on B1's cover with a hardware keyboard: the subtitle made code by
    /// the long press's Make Code, Return types in it, and Tab there indents, typing on, where Tab
    /// in words leaves the text. ⌘Z takes the typing back and ⌘Z again Make Code, so B1 is left
    /// as it was for the tests after this one.
    @MainActor
    func testTabInCodeIndentsAndTypesOn() throws {
        let app = launched()
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        let subtitle = canvas.descendants(matching: .any)
            .matching(NSPredicate(format: "label BEGINSWITH 'Why this exists'")).firstMatch
        guard subtitle.waitForExistence(timeout: 10) else {
            throw Unseen(description: "the cover reads no subtitle: \(app.debugDescription)")
        }
        settle()

        try XCTContext.runActivity(named: "Make Code makes the subtitle code") { _ in
            subtitle.press(forDuration: 1.2)
            let make = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Make Code'")).firstMatch
            guard paged(to: make, in: app) else {
                keep(app, as: "no-make-code")
                throw Unseen(description: "the long press offered no Make Code: \(app.debugDescription)")
            }
            make.tap()
            settle()
            keep(app, as: "made-code")
        }

        let typing = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'typing'")).firstMatch
        try XCTContext.runActivity(named: "Return types in the code, and Tab indents there, typing on") { _ in
            // The subtitle selected by the long press; a newline is the simulator's Return key.
            app.typeKey("\n", modifierFlags: [])
            guard typing.waitForExistence(timeout: 5) else {
                throw Unseen(description: "Return typed in nothing: \(app.debugDescription)")
            }
            app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
            settle(1)
            keep(app, as: "tab-in-code")
            XCTAssertTrue(typing.exists, "Tab in code left the text, as Tab in words does")
            _ = cancel(app, until: { !typing.exists })
        }

        try XCTContext.runActivity(named: "⌘Z takes the typing back, and ⌘Z again Make Code") { _ in
            // B1 is left as it was: Not Code would leave the subtitle in the body's role, not its own.
            app.typeKey("z", modifierFlags: .command)
            settle(1)
            app.typeKey("z", modifierFlags: .command)
            settle()
            subtitle.press(forDuration: 1.2)
            let make = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Make Code'")).firstMatch
            XCTAssertTrue(paged(to: make, in: app), "the subtitle is still code: \(app.debugDescription)")
            app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
        }
    }

    private enum Cancel {
        case escape, commandPeriod, neither
    }

    /// Escape, as a keyboard sends it, until `done` holds; where it does not, ⌘., the system's
    /// cancel key, which the canvas takes as Escape, as a keyboard with no Escape key gives it.
    @MainActor
    private func cancel(_ app: XCUIApplication, until done: () -> Bool) -> Cancel {
        app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
        if wait(5, done) { return .escape }
        app.typeKey(".", modifierFlags: .command)
        return wait(5, done) ? .commandPeriod : .neither
    }

    /// Whether Escape reaches the app at all, said where it did not reach the canvas: the find bar,
    /// opened by ⌘F on the canvas or by the View menu's ⇧⌘F, has a text field of SwiftUI's that
    /// closes the bar on Escape. Closed by its button where Escape does not close it.
    @MainActor
    private func escapeInTheFindBar(_ app: XCUIApplication) -> String {
        let field = app.textFields.matching(NSPredicate(format: "placeholderValue == 'Find in the deck'")).firstMatch
        app.typeKey("f", modifierFlags: .command)
        if !field.waitForExistence(timeout: 3) {
            app.typeKey("f", modifierFlags: [.command, .shift])
            guard field.waitForExistence(timeout: 5) else {
                return "neither ⌘F nor ⇧⌘F opened the find bar to try it in"
            }
        }
        app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
        if wait(5, { !field.exists }) { return "the find bar's text field heard it, and closed" }
        keep(app, as: "find-bar-escape-unheard")
        let close = app.buttons.matching(NSPredicate(format: "label == 'Close the find bar'")).firstMatch
        if close.waitForExistence(timeout: 2) { close.tap() }
        return "nor did the find bar's text field: Escape reached nothing in the app"
    }
}
