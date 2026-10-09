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

    /// B1 in its window: its forty states down the side, the first shown, and Play.
    @MainActor
    func testB1OpensInItsWindow() throws {
        let app = launched()
        try openB1(in: app)
        XCTAssertTrue(app.buttons["Play"].exists, "B1's window has no Play: \(app.debugDescription)")
        settle()
        keep(app, as: "b1")
    }
}
