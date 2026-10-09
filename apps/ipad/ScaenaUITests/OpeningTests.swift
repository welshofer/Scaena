import XCTest

/// The iPad app as someone opens it (PLAN 4.2): launched to the document browser, and B1, which
/// `apps/ipad/app-test.sh` puts in the app's own folder, opened in its window, held landscape as
/// a deck is edited. Each keeps a screenshot of what it shows.
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

    /// B1, opened as the Files app opens a deck in Scaena, or, where this simulator does not open
    /// it so, from the app's own folder in the document browser: its window, its forty states
    /// down the side, the first shown.
    @MainActor
    func testB1OpensInItsWindow() throws {
        let path = try XCTUnwrap(
            ProcessInfo.processInfo.environment["SCAENA_DECK"], "apps/ipad/app-test.sh names B1's place")
        let app = launched()
        app.open(URL(fileURLWithPath: path))
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'cover'")).firstMatch
        if !first.waitForExistence(timeout: 30) {
            let deck = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'b1'")).firstMatch
            if deck.waitForExistence(timeout: 10) { deck.tap() }
        }
        guard first.waitForExistence(timeout: 60) else {
            keep(app, as: "b1-not-shown")
            XCTFail("B1's window did not show: \(app.debugDescription)")
            return
        }
        XCTAssertTrue(app.buttons["Play"].exists, "B1's window has no Play: \(app.debugDescription)")
        settle()
        keep(app, as: "b1")
    }

    /// The app launched, the iPad held landscape.
    @MainActor
    private func launched() -> XCUIApplication {
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication()
        app.launch()
        return app
    }

    /// A moment for what was shown to be painted: the canvas paints at the display's next refresh.
    private func settle() {
        _ = XCTWaiter.wait(for: [XCTestExpectation(description: "painted")], timeout: 2)
    }

    /// A screenshot of `app`, kept with the run's results as `name`.
    @MainActor
    private func keep(_ app: XCUIApplication, as name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
