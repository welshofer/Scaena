import XCTest

/// The iPad app as someone opens it (PLAN 4.2): launched to the document browser, and B1 opened in
/// its window from the browser, held landscape as a deck is edited. Each keeps a screenshot of
/// what it shows.
final class OpeningTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// The app launched with no deck: the document browser, which makes and opens decks.
    @MainActor
    func testAppLaunchesToTheDocumentBrowser() {
        let app = launched()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30), "the app did not come to the foreground")
        settle()
        keep(app, as: "launched")
        XCTAssertEqual(app.state, .runningForeground, "the app did not stay up")
    }

    /// The launch screen offers a new deck, or one written from a prompt (PLAN G.6): New from a
    /// Prompt… shows the brief over the document browser, Generate held until the deck has
    /// something to be about, and Cancel puts it away with no deck made. A deck the app brought
    /// back open is closed first, by its way back to the launch screen.
    @MainActor
    func testNewFromAPromptShowsTheBriefOverTheBrowser() {
        let app = launched()
        let canvas = app.descendants(matching: .any).matching(identifier: "canvas").firstMatch
        if canvas.waitForExistence(timeout: 10) {
            app.buttons.matching(identifier: "BackButton").firstMatch.tap()
            XCTAssertTrue(wait(10) { !canvas.exists }, "the deck brought back did not close: \(app.debugDescription)")
        }
        // The buttons as the launch screen shows them, by their words: the screen also keeps a
        // bare copy of each, out of sight, which a tap passes through.
        let prompting = app.staticTexts.matching(NSPredicate(format: "label == 'New from a Prompt…'")).firstMatch
        XCTAssertTrue(
            prompting.waitForExistence(timeout: 30), "the launch screen offers no New from a Prompt…: \(app.debugDescription)")
        XCTAssertTrue(
            app.staticTexts.matching(NSPredicate(format: "label == 'New Deck'")).firstMatch.exists,
            "the launch screen offers no New Deck: \(app.debugDescription)")
        prompting.tap()
        let topic = app.descendants(matching: .any).matching(identifier: "prompt-topic").firstMatch
        XCTAssertTrue(topic.waitForExistence(timeout: 10), "New from a Prompt… shows no brief: \(app.debugDescription)")
        let generate = app.buttons["prompt-generate"]
        XCTAssertTrue(generate.exists, "the brief has no Generate: \(app.debugDescription)")
        XCTAssertFalse(generate.isEnabled, "Generate is offered with nothing to write about")
        settle()
        keep(app, as: "prompt")
        app.buttons["prompt-cancel"].tap()
        XCTAssertTrue(wait(10) { !topic.exists }, "Cancel left the brief up: \(app.debugDescription)")
        XCTAssertTrue(prompting.waitForExistence(timeout: 10), "Cancel went elsewhere than the launch screen: \(app.debugDescription)")
        XCTAssertFalse(canvas.exists, "Cancel opened a deck: \(app.debugDescription)")
    }

    /// B1 in its window: its forty states down the side, the first shown, and Play.
    @MainActor
    func testB1OpensInItsWindow() throws {
        let app = launched()
        try openB1(in: app)
        XCTAssertTrue(app.buttons["Play"].exists, "B1's window has no Play: \(app.debugDescription)")
        settle()
        keep(app, as: "b1")
        keepElements(app, as: "b1-elements")
    }

    /// A text inserted from the toolbar is typed in, its words selected, as a presentation app's
    /// new text box is (PLAN 3.21).
    @MainActor
    func testATextInsertedIsTypedIn() throws {
        let app = launched()
        try openB1(in: app)
        let text = app.buttons.matching(identifier: "insert-Text").firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 10), "no Text in the toolbar: \(app.debugDescription)")
        text.tap()
        let typing = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Editing'")).firstMatch
        XCTAssertTrue(typing.waitForExistence(timeout: 5), "the text inserted is not typed in: \(app.debugDescription)")
        settle()
        keep(app, as: "inserted")
    }
}
