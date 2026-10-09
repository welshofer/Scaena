import XCTest

/// The assistant on the iPad (PLAN 4.9), on B1: its panel opens, and with no key kept it offers to
/// add one. The keys' sheet keeps a key in the iPad's Keychain, keeps another in its place, and
/// takes it away, each tap its own button's; with a key kept, the panel offers none to add.
final class AssistantTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testAKeyIsKeptInTheIPadsKeychain() throws {
        let app = launched()
        try openB1(in: app)
        // The toolbar's Assistant where it shows, else its key.
        let toggle = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Assistant'")).firstMatch
        if toggle.waitForExistence(timeout: 5), toggle.isHittable {
            toggle.tap()
        } else {
            app.typeKey("a", modifierFlags: [.command, .option])
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
        let remove = app.buttons.matching(NSPredicate(format: "identifier == 'remove-anthropic'")).firstMatch
        let kept = { (field.placeholderValue ?? "") == "Kept in the Keychain" }

        XCTContext.runActivity(named: "A key is kept, and another in its place") { _ in
            field.tap()
            field.typeText("sk-ant-scaena-test-1")
            keepIt.tap()
            XCTAssertTrue(wait(5) { kept() && remove.isEnabled }, "the key is not kept: \(field.placeholderValue ?? "")")
            keep(app, as: "key-kept")
            // Keep's tap is Keep's alone: a row's buttons each take their own.
            field.tap()
            field.typeText("sk-ant-scaena-test-2")
            keepIt.tap()
            XCTAssertTrue(wait(5) { kept() }, "keeping another key took it away: \(field.placeholderValue ?? "")")
        }

        XCTContext.runActivity(named: "With a key kept, the panel offers none to add") { _ in
            app.buttons.matching(NSPredicate(format: "label == 'Done'")).firstMatch.tap()
            XCTAssertTrue(wait(10) { !add.exists }, "the panel still offers a key to add")
        }

        XCTContext.runActivity(named: "Taken away, the Keychain is as it was") { _ in
            app.buttons.matching(NSPredicate(format: "identifier == 'assistant-options'")).firstMatch.tap()
            app.buttons.matching(NSPredicate(format: "label == 'Keys…'")).firstMatch.tap()
            XCTAssertTrue(remove.waitForExistence(timeout: 10), "the keys did not show again")
            remove.tap()
            XCTAssertTrue(wait(5) { !kept() && !remove.isEnabled }, "the key stayed: \(field.placeholderValue ?? "")")
            app.buttons.matching(NSPredicate(format: "label == 'Done'")).firstMatch.tap()
            XCTAssertTrue(add.waitForExistence(timeout: 10), "with no key kept, the panel offers none to add")
        }
    }
}
