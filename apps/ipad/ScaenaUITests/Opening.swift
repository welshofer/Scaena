import XCTest

/// What a test looked for and did not find, with what the app showed: thrown, it fails the test.
struct Unseen: Error, CustomStringConvertible {
    let description: String
}

/// What every test of the app does first: launch it, open B1, and keep what it shows.
extension XCTestCase {
    /// The app launched, the iPad held landscape, as a deck is edited.
    @MainActor
    func launched() -> XCUIApplication {
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication()
        app.launch()
        return app
    }

    /// B1, which `apps/ipad/app-test.sh` puts in the app's own folder, open in its window, as
    /// someone opens it: in the document browser, Browse, On My iPad, the app's folder, then B1.
    /// Its first state, `cover`, is shown, found by its id in the slides down the side; a window the
    /// app brought back already showing it is taken as it is.
    @MainActor
    func openB1(in app: XCUIApplication) throws {
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'state-cover'")).firstMatch
        if first.waitForExistence(timeout: 5) { return }
        let b1 = cell(in: app, named: "b1")
        let folder = cell(in: app, named: "Scaena")
        let onIPad = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'On My iPad'")).firstMatch
        let browse = app.buttons.matching(NSPredicate(format: "label == 'Browse'")).firstMatch
        // A step at a time, from wherever the browser stands: each tap goes one place further. A tap
        // on B1 the browser lets go by, as it may while it reads the folder again after the app is
        // installed, is made again.
        for _ in 0..<8 {
            if b1.waitForExistence(timeout: 3) {
                b1.tap()
                if first.waitForExistence(timeout: 15) { return }
                continue
            }
            if folder.exists {
                folder.tap()
            } else if onIPad.exists {
                onIPad.tap()
            } else if browse.exists {
                browse.tap()
            }
        }
        guard first.waitForExistence(timeout: 60) else {
            keep(app, as: "b1-not-shown")
            throw Unseen(description: "B1's window did not show: \(app.debugDescription)")
        }
    }

    /// The document browser's item named `name`: a file or a folder, by the name it shows first.
    @MainActor
    private func cell(in app: XCUIApplication, named name: String) -> XCUIElement {
        app.cells.matching(NSPredicate(format: "label BEGINSWITH %@", name)).firstMatch
    }

    /// Whether `holds` comes to hold within `seconds`, asked every tenth of a second.
    @MainActor
    func wait(_ seconds: TimeInterval, _ holds: () -> Bool) -> Bool {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            if holds() { return true }
            _ = XCTWaiter.wait(for: [XCTestExpectation(description: "a tenth")], timeout: 0.1)
        }
        return holds()
    }

    /// A moment for what was shown to be painted: the canvas paints at the display's next refresh.
    func settle(_ seconds: TimeInterval = 2) {
        _ = XCTWaiter.wait(for: [XCTestExpectation(description: "painted")], timeout: seconds)
    }

    /// A screenshot of `app`, kept with the run's results as `name`.
    @MainActor
    func keep(_ app: XCUIApplication, as name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
