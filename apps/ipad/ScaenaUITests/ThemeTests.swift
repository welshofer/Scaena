import XCTest

/// The theme's values on the iPad (PLAN 3.28), on B1: the Document tab's Theme shows each value in
/// a field that reads as one. A size typed is set on Return, and the role's row says the size the
/// theme now gives it; ⌘Z takes it back and ⌘⇧Z makes it again, each one step; a stepper's tap is
/// one step too. B1 is left as it was, and the inspector on its Format tab: the tests after this one
/// open it too.
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

        let size = app.textFields.matching(NSPredicate(format: "identifier == 'theme-body-size'")).firstMatch
        try XCTContext.runActivity(named: "The body role opened: its size in a field") { _ in
            let body = any.matching(NSPredicate(format: "label BEGINSWITH 'Body'")).firstMatch
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
            size.tap()
            app.typeKey("a", modifierFlags: .command)
            // Return as the simulator's keyboard sends it: a newline.
            size.typeText("37\n")
            guard says(37).waitForExistence(timeout: 10) else {
                keep(app, as: "size-not-set")
                throw Unseen(description: "the body role does not say 37 pt: \(app.debugDescription)")
            }
            XCTAssertEqual(size.value as? String, "37")
            keep(app, as: "size-typed")
            app.typeKey("z", modifierFlags: .command)
            XCTAssertTrue(says(32).waitForExistence(timeout: 10), "⌘Z did not take the size back")
            XCTAssertTrue(wait(5) { size.value as? String == "32" }, "the field shows \(size.value ?? "nothing"), not 32")
            keep(app, as: "size-undone")
            app.typeKey("z", modifierFlags: [.command, .shift])
            XCTAssertTrue(says(37).waitForExistence(timeout: 10), "⌘⇧Z did not make the size again")
            app.typeKey("z", modifierFlags: .command)
            XCTAssertTrue(says(32).waitForExistence(timeout: 10), "⌘Z did not take the size back again")
        }

        try XCTContext.runActivity(named: "A stepper's tap is one step to undo") { _ in
            // By its identifier, else the form's first: the body role's, the only one opened.
            let identified = any.matching(NSPredicate(format: "identifier == 'theme-body-size-stepper'")).firstMatch
            let stepper = identified.exists ? identified : app.steppers.firstMatch
            let more = stepper.buttons.matching(NSPredicate(format: "label == 'Increment'")).firstMatch
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

        // The inspector as the other tests find it.
        if tab("Format").exists { tab("Format").tap() }
    }
}
