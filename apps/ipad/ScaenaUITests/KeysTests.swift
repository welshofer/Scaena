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
            // A chord types nothing in a text typed in (lift-up 20261110): ⌘D there once typed a
            // "d" over the title's words, as the system hands a chord's characters on.
            let cover = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'state-cover'")).firstMatch
            let read = cover.label
            app.typeKey("d", modifierFlags: .command)
            if wait(3, { cover.label != read }) {
                keep(app, as: "chord-typed")
                XCTFail("⌘D typed in the title: the cover read \(read), now \(cover.label)")
            }
            XCTAssertTrue(typing.exists, "⌘D stopped typing: \(app.debugDescription)")
            switch cancel(app, until: { !typing.exists && title.isSelected }) {
            case .escape: break
            case .commandPeriod: cancelled.append("while typing")
            case .neither:
                keep(app, as: "typing-not-stopped")
                unheard.append("neither Escape nor ⌘. stopped typing")
                // Typing stopped as a tap stops it, where nothing draws, and the title selected again:
                // at the slide's top, which the software keyboard leaves clear (lift-up 20261110).
                canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.03)).tap()
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
            pressForMenu(subtitle, in: app)
            let make = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Make Code'")).firstMatch
            guard paged(to: make, in: app) else {
                keep(app, as: "no-make-code")
                // Copy and Duplicate come with a node selected: without them the menu was made for nothing.
                let copy = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Copy'")).firstMatch
                let what = copy.exists ? "no Make Code" : "nothing done to the subtitle, no Copy or Duplicate"
                throw Unseen(description: "the long press offered \(what): \(app.debugDescription)")
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
            // Each ⌘Z is waited on by the step the toolbar's Undo names, never by a fixed pause: on a
            // loaded runner the burst of typing ends, and a ⌘Z lands, later than a second allows,
            // and two ⌘Z sent blind left the subtitle code (PLAN 4.11, the undo's own words).
            let undo = app.navigationBars.buttons.matching(NSPredicate(format: "identifier == 'undo'")).firstMatch
            guard undo.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no Undo in the toolbar: \(app.debugDescription)")
            }
            XCTAssertTrue(wait(5) { undo.label == "Undo Typing" }, "the step to take back first is \(undo.label)")
            try undoing(undo, in: app)
            XCTAssertEqual(undo.label, "Undo Make Code", "the typing was not one step: \(app.debugDescription)")
            try undoing(undo, in: app)
            pressForMenu(subtitle, in: app)
            let make = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Make Code'")).firstMatch
            XCTAssertTrue(paged(to: make, in: app), "the subtitle is still code: \(app.debugDescription)")
            app.typeKey(XCUIKeyboardKey.escape, modifierFlags: [])
        }
    }

    /// Typing in code with the software keyboard up, the app settles: nothing on screen animates
    /// on with nothing changing, so XCTest's wait for the app to go idle before an event, which
    /// it makes before every key and tap, comes to an end at once. On #865's iPad run it never
    /// did: each event waited out the quiescence timeout, some two minutes, and the test ran past
    /// its ten (lift-up 20261110). The app is launched with `-reportAnimations YES`, which says in
    /// `log.txt` every two seconds what animates while a text is typed in.
    @MainActor
    func testTypingInCodeSettles() throws {
        let app = launched(arguments: ["-reportAnimations", "YES"])
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        let subtitle = canvas.descendants(matching: .any)
            .matching(NSPredicate(format: "label BEGINSWITH 'Why this exists'")).firstMatch
        guard subtitle.waitForExistence(timeout: 10) else {
            throw Unseen(description: "the cover reads no subtitle: \(app.debugDescription)")
        }
        settle()
        pressForMenu(subtitle, in: app)
        let make = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Make Code'")).firstMatch
        guard paged(to: make, in: app) else {
            keep(app, as: "settles-no-make-code")
            throw Unseen(description: "the long press offered no Make Code: \(app.debugDescription)")
        }
        make.tap()
        settle()
        let typing = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'typing'")).firstMatch
        app.typeKey("\n", modifierFlags: [])
        guard typing.waitForExistence(timeout: 5) else {
            throw Unseen(description: "Return typed in nothing: \(app.debugDescription)")
        }
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
        // What a tab typed and the keyboard brought have had time to finish.
        settle(3)
        keep(app, as: "settles-typing")
        // → at the end of the line moves nothing: what it takes is XCTest's wait for the app to
        // settle before it, which is a moment where nothing animates on.
        let began = Date()
        app.typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: [])
        let waited = Date().timeIntervalSince(began)
        print("a key typed in code waited \(String(format: "%.1f", waited)) s for the app to settle")
        XCTAssertLessThan(waited, 30, "typing in code, the app did not settle: a key waited \(Int(waited)) s")
        XCTAssertTrue(typing.exists, "→ at the end of the code left the text")
        _ = cancel(app, until: { !typing.exists })
    }

    /// ⌘Z, waited on until the toolbar's Undo names another step than the one it named: the key
    /// sent again once where the app did not hear it, as a loaded simulator drops one.
    @MainActor
    private func undoing(_ undo: XCUIElement, in app: XCUIApplication) throws {
        let step = undo.label
        for _ in 0..<2 {
            app.typeKey("z", modifierFlags: .command)
            if wait(5, { undo.label != step }) {
                settle(1)
                return
            }
        }
        throw Unseen(description: "⌘Z took back nothing: Undo still reads \(step): \(app.debugDescription)")
    }

    /// ⌘Z with the long press's menu up closes the menu and undoes, as UIKit's own menus take a
    /// hardware keyboard's keys (lift-up 20261110): the title copied by ⌘D, the menu asked of it,
    /// and ⌘Z takes the copy back with the menu gone. With the menu holding the keyboard, ⌘Z went
    /// nowhere and ThemeTests waited out its two minutes.
    @MainActor
    func testUndoKeyClosesTheMenuAndUndoes() throws {
        let app = launched()
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        let titles = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'"))
        let title = titles.firstMatch
        guard title.waitForExistence(timeout: 10) else {
            throw Unseen(description: "the cover reads no title: \(app.debugDescription)")
        }
        settle()
        // Selected by keys, as testTheCanvasByKeysAlone selects it: a tap on the title can reach the
        // simulator's canvas as two and begin typing in it, where ⌘D types a "d" (lift-up 20261110).
        canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
        guard wait(5, { title.isSelected }) else {
            keep(app, as: "undo-key-unselected")
            throw Unseen(description: "Tab selected no title: \(app.debugDescription)")
        }
        app.typeKey("d", modifierFlags: .command)
        guard wait(5, { titles.count == 2 }) else {
            keep(app, as: "undo-key-no-copy")
            throw Unseen(description: "⌘D made no copy: \(app.debugDescription)")
        }
        settle()
        pressForMenu(title, in: app)
        let copy = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Copy'")).firstMatch
        guard wait(5, { copy.isHittable }) else {
            keep(app, as: "undo-key-no-menu")
            throw Unseen(description: "a long press offered no menu: \(app.debugDescription)")
        }
        keep(app, as: "undo-key-menu")
        app.typeKey("z", modifierFlags: .command)
        guard wait(10, { titles.count == 1 }) else {
            keep(app, as: "undo-key-unheard")
            throw Unseen(description: "⌘Z with the menu up took nothing back: \(app.debugDescription)")
        }
        XCTAssertTrue(wait(5) { !copy.isHittable }, "the menu stayed up after ⌘Z: \(app.debugDescription)")
        keep(app, as: "undo-key-closed")
        settle()
    }

    /// Undo and Redo held while the assistant is at work on the deck (lift-up STAB-5), as the
    /// Mac's Edit menu holds them: on B1's cover with a hardware keyboard, the title nudged by
    /// eight arrows, the deck then held as the assistant holds it (`-assistantAtWorkAfter 8`), and ⌘Z and ⇧⌘Z
    /// take nothing back, the canvas saying the assistant is at work on the deck.
    @MainActor
    func testUndoWaitsWhileTheAssistantIsAtWork() throws {
        let app = launched(arguments: ["-assistantAtWorkAfter", "8"])
        try openB1(in: app)
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        let title = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'")).firstMatch
        guard title.waitForExistence(timeout: 10) else {
            throw Unseen(description: "the cover reads no title: \(app.debugDescription)")
        }
        let hint = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'hint'")).firstMatch
        let said = { hint.exists ? hint.label : "nothing" }
        settle()

        // A tap where nothing draws gives the canvas the keyboard; Tab selects the title, and eight
        // arrows nudge it, the eight edits after which the deck is held.
        canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
        guard wait(5, { title.isSelected }) else {
            throw Unseen(description: "Tab selected no title: \(app.debugDescription)")
        }
        let before = title.frame
        for _ in 0..<8 {
            app.typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: [])
        }
        guard wait(5, { title.frame.minX > before.minX + 2 }) else {
            throw Unseen(description: "the arrows nudged nothing: \(before) → \(title.frame)")
        }
        let nudged = title.frame
        settle(1)

        try XCTContext.runActivity(named: "⌘Z and ⇧⌘Z take nothing back, saying the assistant is at work") { _ in
            app.typeKey("z", modifierFlags: .command)
            settle(1)
            keep(app, as: "undo-held")
            XCTAssertTrue(wait(5) { said().hasPrefix("The assistant is at work on the deck") }, "⌘Z says \(said())")
            XCTAssertEqual(
                title.frame.minX, nudged.minX, accuracy: 0.5, "⌘Z took a nudge back while the assistant worked")
            // The hint stands six seconds, and ⇧⌘Z says the same words, which keep it standing no
            // longer: so ⇧⌘Z is pressed once ⌘Z's words have gone, and must bring them back. Pressed
            // within those seconds, as the first run of this test did, it was held (the keys log
            // says so) but its words went with ⌘Z's before they were read.
            XCTAssertTrue(wait(10) { !hint.exists }, "⌘Z's words stand: \(said())")
            app.typeKey("z", modifierFlags: [.command, .shift])
            XCTAssertTrue(wait(5) { said().hasPrefix("The assistant is at work on the deck") }, "⇧⌘Z says \(said())")
            settle(1)
            XCTAssertEqual(
                title.frame.minX, nudged.minX, accuracy: 0.5, "⇧⌘Z changed the title while the assistant worked")
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
