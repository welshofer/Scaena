import XCTest

/// The assistant on the iPad (PLAN 4.9), on B1: its panel opens, and with no key kept it offers to
/// add one. The keys' sheet checks a key with its provider before it keeps it in the iPad's
/// Keychain, shows the key kept masked, keeps another in its place, and takes it away when asked
/// twice, each tap its own button's; with a key kept, the panel offers none to add. The provider
/// is stood in for (`SCAENA_KEY_CHECK`): a test sends no key anywhere.
final class AssistantTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testAKeyIsCheckedAndKeptInTheIPadsKeychain() throws {
        let app = launched(environment: ["SCAENA_KEY_CHECK": "accept"])
        try openB1(in: app)
        // The window is the deck's once its canvas shows: on a slow simulator the slide list shows
        // while the iPad still draws the document opening, and a press then is lost.
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        _ = canvas.waitForExistence(timeout: 10)
        settle()
        // The toolbar's Assistant where it shows, else its key; pressed again only where the panel
        // did not open, since a second press puts an open one away.
        let toggle = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Assistant'")).firstMatch
        let panel = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'assistant-options'")).firstMatch
        for attempt in 1...2 where !panel.exists {
            if toggle.waitForExistence(timeout: 5), toggle.isHittable {
                toggle.tap()
            } else {
                app.typeKey("a", modifierFlags: [.command, .option])
            }
            if !panel.waitForExistence(timeout: 10), attempt == 1 {
                keep(app, as: "assistant-unopened")
            }
        }
        let add = app.buttons.matching(NSPredicate(format: "identifier == 'add-key'")).firstMatch
        guard add.waitForExistence(timeout: 10) else {
            keep(app, as: "no-assistant")
            throw Unseen(description: "the assistant offers no key to add: \(app.debugDescription)")
        }
        keep(app, as: "assistant")
        add.tap()

        let field = app.secureTextFields.matching(NSPredicate(format: "identifier == 'key-anthropic'")).firstMatch
        guard field.waitForExistence(timeout: 10) else {
            keep(app, as: "no-keys")
            throw Unseen(description: "Add a Key… showed no keys: \(app.debugDescription)")
        }
        let keepIt = app.buttons.matching(NSPredicate(format: "identifier == 'keep-anthropic'")).firstMatch
        let replace = app.buttons.matching(NSPredicate(format: "identifier == 'replace-anthropic'")).firstMatch
        let remove = app.buttons.matching(NSPredicate(format: "identifier == 'remove-anthropic'")).firstMatch
        let kept = app.staticTexts.matching(NSPredicate(format: "identifier == 'kept-anthropic'")).firstMatch
        let hint = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'hint-anthropic'")).firstMatch

        XCTContext.runActivity(named: "A key of another form is said so as it is typed") { _ in
            field.tap()
            field.typeText("AIzaSyA-not-an-anthropic-key")
            XCTAssertTrue(hint.waitForExistence(timeout: 5), "a Gemini key typed for Anthropic is not said so")
            keep(app, as: "key-hinted")
            // Taken back, the hint goes.
            field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 40))
            XCTAssertTrue(wait(5) { !hint.exists }, "the hint stays with the field empty")
        }

        XCTContext.runActivity(named: "A key is checked, kept, and shown masked; another is kept in its place") { _ in
            field.typeText("sk-ant-scaena-test-1")
            keepIt.tap()
            XCTAssertTrue(wait(5) { kept.exists && kept.label.hasSuffix("st-1") }, "the key is not kept: \(app.debugDescription)")
            XCTAssertFalse(field.exists, "a key kept shows masked, not in a field")
            keep(app, as: "key-kept")
            // Each provider says what its key is for, as a sentence would (PLAN 3.18).
            let openai = app.descendants(matching: .any)
                .matching(NSPredicate(format: "label CONTAINS 'GPT and pictures'")).firstMatch
            XCTAssertTrue(openai.exists, "OpenAI's key does not say what it is for: \(app.debugDescription)")
            // Replace's tap is Replace's alone: a row's buttons each take their own.
            replace.tap()
            XCTAssertTrue(field.waitForExistence(timeout: 5), "Replace… shows no field")
            field.tap()
            field.typeText("sk-ant-scaena-test-2")
            keepIt.tap()
            XCTAssertTrue(wait(5) { kept.exists && kept.label.hasSuffix("st-2") }, "the key kept in its place is not shown: \(kept.label)")
        }

        XCTContext.runActivity(named: "With a key kept, the panel offers none to add") { _ in
            app.buttons.matching(NSPredicate(format: "label == 'Done'")).firstMatch.tap()
            XCTAssertTrue(wait(10) { !add.exists }, "the panel still offers a key to add")
        }

        XCTContext.runActivity(named: "Taken away when asked twice, the Keychain is as it was") { _ in
            app.buttons.matching(NSPredicate(format: "identifier == 'assistant-options'")).firstMatch.tap()
            app.buttons.matching(NSPredicate(format: "label == 'Keys…'")).firstMatch.tap()
            XCTAssertTrue(remove.waitForExistence(timeout: 10), "the keys did not show again")
            remove.tap()
            let confirmed = app.buttons.matching(NSPredicate(format: "label == 'Remove Key'")).firstMatch
            XCTAssertTrue(confirmed.waitForExistence(timeout: 5), "Remove… does not ask before it takes the key away")
            XCTAssertTrue(kept.exists, "the key went before Remove Key was tapped")
            keep(app, as: "key-removing")
            confirmed.tap()
            XCTAssertTrue(wait(5) { !kept.exists && field.exists }, "the key stayed: \(app.debugDescription)")
            app.buttons.matching(NSPredicate(format: "label == 'Done'")).firstMatch.tap()
            XCTAssertTrue(add.waitForExistence(timeout: 10), "with no key kept, the panel offers none to add")
        }
    }
}
