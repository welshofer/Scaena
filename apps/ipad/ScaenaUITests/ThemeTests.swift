import XCTest

/// The theme's values on the iPad (PLAN 3.28), on B1: the Document tab's Theme shows each value in
/// a field that reads as one. A field tapped selects its value, so a size typed takes its place, and
/// is set on Return: the role's row says the size the theme now gives it. ⌘Z takes it back and ⌘⇧Z
/// makes it again, each one step; a stepper's tap is one step too. B1 is left as it was, and the
/// inspector on its Format tab: the tests after this one open it too.
final class ThemeTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testAThemeValueTypedIsOneStepToUndo() throws {
        let app = launched()
        try openB1(in: app)
        let any = app.descendants(matching: .any)
        let tab = { (title: String) in
            app.segmentedControls.buttons.matching(NSPredicate(format: "label == %@", title)).firstMatch
        }
        // The body role's size as its row says it, `Body, 32 pt`: the theme as it is now, not what
        // was typed.
        let says = { (size: Int) in
            any.matching(NSPredicate(format: "label MATCHES %@", "(.*[^0-9])?\(size) pt.*")).firstMatch
        }
        // The cover's title, which Tab selects where the canvas has the keys and nothing is selected.
        let title = any.matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
            .descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'")).firstMatch

        try XCTContext.runActivity(named: "The Document tab shows the theme") { _ in
            if !tab("Document").waitForExistence(timeout: 5) {
                // The inspector put away: the toolbar's Document shows it at that tab.
                app.buttons.matching(NSPredicate(format: "label == 'Document'")).firstMatch.tap()
            }
            guard tab("Document").waitForExistence(timeout: 10) else {
                keep(app, as: "no-document-tab")
                throw Unseen(description: "the inspector shows no Document tab: \(app.debugDescription)")
            }
            tab("Document").tap()
            guard tab("Theme").waitForExistence(timeout: 10) else {
                keep(app, as: "no-theme-panel")
                throw Unseen(description: "the Document tab offers no Theme: \(app.debugDescription)")
            }
            tab("Theme").tap()
        }

        try XCTContext.runActivity(named: "Each color's name and what it is set in read whole, beside its field") { _ in
            // The paper's row: what the theme sets in it, the background and text on the accent,
            // stands under its name on as many lines as it needs, clear of its field, which is as
            // wide as the accent's; two lines beside a spacer cut it short, "Text on Acc…".
            let identified = { (id: String) in NSPredicate(format: "identifier == %@", id) }
            let name = app.staticTexts.matching(identified("theme-name-paper")).firstMatch
            let uses = app.staticTexts.matching(identified("theme-uses-paper")).firstMatch
            let field = app.textFields.matching(identified("theme-color-paper")).firstMatch
            let accent = app.textFields.matching(identified("theme-color-accent")).firstMatch
            guard uses.waitForExistence(timeout: 10), name.exists, field.exists, accent.exists else {
                keep(app, as: "no-paper-uses")
                throw Unseen(description: "the theme's colors show no paper with its uses: \(app.debugDescription)")
            }
            keep(app, as: "theme-colors")
            XCTAssertEqual(uses.label, "Background, Text on Accent")
            XCTAssertGreaterThanOrEqual(uses.frame.minY, name.frame.maxY - 1, "the uses stand under the name")
            XCTAssertLessThanOrEqual(uses.frame.maxX, field.frame.minX, "the uses stand clear of the field")
            XCTAssertEqual(field.frame.width, accent.frame.width, accuracy: 0.5, "the paper's field is as wide as any")
        }

        let size = app.textFields.matching(NSPredicate(format: "identifier == 'theme-body-size'")).firstMatch
        try XCTContext.runActivity(named: "The body role opened: its size in a field") { _ in
            let body = any.matching(NSPredicate(format: "label BEGINSWITH 'Body'")).firstMatch
            // The type roles come in the order the theme writes them, and Display, Headline, and
            // Title, before Body, reach the window's foot: a form makes no row it does not show,
            // nor the fields a row opens below the foot. So the panel is scrolled until Body
            // stands clear of the foot, and of the fields it opens below it, as a person scrolls
            // it. Neither a drag from the first role's row nor one from the panel's right margin
            // scrolled it on CI's simulator: Body stood at y 1017–1069 in a window 1032 tall, the
            // panel's scroll bar at 0%.
            let foot = app.windows.firstMatch.frame.maxY
            let clear = { body.exists && body.isHittable && body.frame.maxY < foot - 240 }
            _ = body.waitForExistence(timeout: 10)
            let moves = scrollPanel(in: app, to: body, tries: 6, until: clear)
            guard clear() else {
                keep(app, as: "body-role-unreached")
                throw Unseen(
                    description: "the body role never stood clear of the window's foot at \(foot): after "
                        + "\(moves.isEmpty ? "no move" : moves.joined(separator: ", ")) it stands at "
                        + "\(body.exists ? "\(body.frame)" : "nowhere"), \(scrolled(app)): "
                        + "\(app.debugDescription)")
            }
            guard body.waitForExistence(timeout: 10), says(32).exists else {
                keep(app, as: "no-body-role")
                throw Unseen(description: "the theme shows no body role at 32 pt: \(app.debugDescription)")
            }
            body.tap()
            guard size.waitForExistence(timeout: 10) else {
                keep(app, as: "no-size-field")
                throw Unseen(description: "the body role shows no size field: \(app.debugDescription)")
            }
            XCTAssertEqual(size.value as? String, "32")
            keep(app, as: "theme")
        }

        try XCTContext.runActivity(named: "A size typed is set on Return: ⌘Z takes it back, ⌘⇧Z makes it again") { _ in
            // Tapped as a person taps it, left of the number: the field selects its value, so what
            // is typed takes its place. Typed key by key, as the keyboard ⌘Z is pressed on types
            // it: after text the simulator's own keyboard typed (`typeText`), no key reached the
            // canvas. Return as that keyboard sends it: a newline.
            size.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.5)).tap()
            settle(1)
            for key in ["3", "7", "\n"] { app.typeKey(key, modifierFlags: []) }
            guard says(37).waitForExistence(timeout: 10) else {
                keep(app, as: "size-not-set")
                throw Unseen(description: "the body role does not say 37 pt: \(app.debugDescription)")
            }
            XCTAssertEqual(size.value as? String, "37")
            keep(app, as: "size-typed")
            app.typeKey("z", modifierFlags: .command)
            guard says(32).waitForExistence(timeout: 10) else {
                keep(app, as: "size-not-undone")
                // Whether any key reaches the canvas now: with nothing selected, Tab selects the
                // cover's title.
                app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
                let tab =
                    wait(5) { title.isSelected }
                    ? "selected the title: the canvas has the keys" : "selected nothing: no key reached the canvas"
                throw Unseen(description: "⌘Z did not take the size back; Tab \(tab): \(app.debugDescription)")
            }
            XCTAssertTrue(wait(5) { size.value as? String == "32" }, "the field shows \(size.value ?? "nothing"), not 32")
            keep(app, as: "size-undone")
            app.typeKey("z", modifierFlags: [.command, .shift])
            XCTAssertTrue(says(37).waitForExistence(timeout: 10), "⌘⇧Z did not make the size again")
            app.typeKey("z", modifierFlags: .command)
            XCTAssertTrue(says(32).waitForExistence(timeout: 10), "⌘Z did not take the size back again")
        }

        try XCTContext.runActivity(named: "A stepper's tap is one step to undo") { _ in
            let more = any.matching(NSPredicate(format: "identifier == 'theme-body-size-stepper-Increment'")).firstMatch
            guard more.waitForExistence(timeout: 5) else {
                keep(app, as: "no-stepper")
                throw Unseen(description: "the size has no stepper: \(app.debugDescription)")
            }
            more.tap()
            XCTAssertTrue(says(33).waitForExistence(timeout: 10), "the stepper's tap did not set 33 pt")
            keep(app, as: "size-stepped")
            app.typeKey("z", modifierFlags: .command)
            XCTAssertTrue(says(32).waitForExistence(timeout: 10), "⌘Z did not take the step back")
        }

        try XCTContext.runActivity(named: "Code's keywords named a color are one step to undo") { _ in
            // The Code section stands under Spacing, near the foot of a panel some pages long:
            // scrolled to as a person scrolls, until the keywords' row shows (PLAN 2.144).
            let keywords = app.staticTexts.matching(NSPredicate(format: "identifier == 'theme-code-keyword'")).firstMatch
            let derived = any.matching(NSPredicate(format: "label == 'From the accent'")).firstMatch
            let moves = scrollPanel(in: app, to: keywords, tries: 12) { keywords.exists && keywords.isHittable }
            guard keywords.waitForExistence(timeout: 10), derived.exists else {
                keep(app, as: "no-code-colors")
                throw Unseen(
                    description: "the theme shows no code colors after \(moves.joined(separator: ", ")), "
                        + "\(scrolled(app)): "
                        + "\(app.debugDescription)")
            }
            keep(app, as: "theme-code")
            let pick = any.matching(NSPredicate(format: "identifier == 'theme-code-color-keyword'")).firstMatch
            guard pick.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the keywords have no color to pick: \(app.debugDescription)")
            }
            pick.tap()
            let muted = app.buttons.matching(NSPredicate(format: "label == 'Muted Text'")).firstMatch
            guard muted.waitForExistence(timeout: 5) else {
                keep(app, as: "no-code-menu")
                throw Unseen(description: "the keywords' colors offer no Muted Text: \(app.debugDescription)")
            }
            muted.tap()
            XCTAssertTrue(wait(10) { "\(pick.label) \(pick.value as? String ?? "")".contains("Muted Text") }, "the keywords are not Muted Text: \(pick.debugDescription)")
            keep(app, as: "theme-code-named")
            app.typeKey("z", modifierFlags: .command)
            XCTAssertTrue(wait(10) { "\(pick.label) \(pick.value as? String ?? "")".contains("From the Accent") }, "⌘Z did not take the keywords' color back: \(pick.debugDescription)")
        }

        // The inspector as the other tests find it.
        if tab("Format").exists { tab("Format").tap() }
    }
}

/// Scrolls the Theme panel, as a person scrolls it, until `shown` holds, at most `tries` moves,
/// and says each move it made. A move swipes the panel's list itself, as XCUITest scrolls a
/// list: on CI's simulator a drag from the panel's right margin, which lies on the scroll bar's
/// own column, moved nothing, nor did one from a row's button. Where a swipe moves nothing, as
/// its scroll bar says, the next move is that drag, held still before it is let go, and the
/// other way round. Where `target` has gone above the list's top, the move goes down.
@MainActor
private func scrollPanel(
    in app: XCUIApplication, to target: XCUIElement, tries: Int, until shown: () -> Bool
) -> [String] {
    let window = app.windows.firstMatch.frame
    let margin = app.windows.firstMatch.coordinate(withNormalizedOffset: .zero)
        .withOffset(CGVector(dx: window.maxX - 55, dy: window.maxY * 0.75))
    var moves: [String] = []
    var swiping = true
    while moves.count < tries && !shown() {
        let before = scrolled(app)
        let list = panelList(in: app)
        let top = max(list?.frame.minY ?? window.minY, window.minY)
        let down = target.exists && target.frame.minY < top
        if swiping, let list {
            if down { list.swipeDown(velocity: .slow) } else { list.swipeUp(velocity: .slow) }
            moves.append(down ? "swiped down" : "swiped up")
        } else {
            margin.press(
                forDuration: 0.05, thenDragTo: margin.withOffset(CGVector(dx: 0, dy: down ? 400 : -400)),
                withVelocity: .slow, thenHoldForDuration: 0.5)
            moves.append(down ? "dragged down" : "dragged up")
        }
        // A moment for a swipe's glide to stop before the scroll bar is read.
        _ = XCTWaiter.wait(for: [XCTestExpectation(description: "scrolled")], timeout: 1)
        if scrolled(app) == before { swiping.toggle() }
    }
    return moves
}

/// The Theme panel's list: of the window's collection views, the one in its right half, where
/// the inspector stands; none where none stands there.
@MainActor
private func panelList(in app: XCUIApplication) -> XCUIElement? {
    let window = app.windows.firstMatch.frame
    let lists = app.collectionViews
    let at = (0..<lists.count).first { i in
        let frame = lists.element(boundBy: i).frame
        return frame.minX > window.midX && frame.minY < window.maxY
    }
    return at.map { lists.element(boundBy: $0) }
}

/// How far the panel has scrolled, as its scroll bar says it.
@MainActor
private func scrolled(_ app: XCUIApplication) -> String {
    let bar = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'Vertical scroll bar'"))
    let said = (0..<bar.count).map { i -> String in
        let one = bar.element(boundBy: i)
        return "\(one.label) at \(one.value as? String ?? "?")"
    }
    return said.isEmpty ? "no scroll bar shown" : said.joined(separator: "; ")
}
