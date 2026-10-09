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
    ///
    /// B1 is tapped once, then given a minute. On the first launch after the app is installed, a
    /// tap on B1 has gone unanswered for minutes, and B1 tapped again changed nothing: there the
    /// app is launched again, once, and B1 tapped in it. What the app says of its opening (the
    /// GPU, the deck read, the canvas's surface) is kept in `log.txt` beside the screenshots.
    @MainActor
    func openB1(in app: XCUIApplication) throws {
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'state-cover'")).firstMatch
        if first.waitForExistence(timeout: 5) { return }
        for launch in 1...2 {
            if launch > 1 {
                keep(app, as: "b1-unanswered")
                app.terminate()
                app.launch()
                if first.waitForExistence(timeout: 5) { return }
            }
            try XCTContext.runActivity(named: "B1 tapped in the document browser, launch \(launch)") { _ in
                try tapB1(in: app)
            }
            if first.waitForExistence(timeout: 60) { return }
        }
        keep(app, as: "b1-not-shown")
        throw Unseen(description: "B1's window did not show: \(app.debugDescription)")
    }

    /// The document browser walked to B1, a step at a time from wherever it stands, each tap one
    /// place further, and B1 tapped.
    @MainActor
    private func tapB1(in app: XCUIApplication) throws {
        let b1 = cell(in: app, named: "b1")
        let folder = cell(in: app, named: "Scaena")
        let onIPad = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'On My iPad'")).firstMatch
        let browse = app.buttons.matching(NSPredicate(format: "label == 'Browse'")).firstMatch
        for _ in 0..<8 {
            if b1.waitForExistence(timeout: 3) {
                b1.tap()
                return
            }
            if folder.exists {
                folder.tap()
            } else if onIPad.exists {
                onIPad.tap()
            } else if browse.exists {
                browse.tap()
            }
        }
        keep(app, as: "b1-not-found")
        throw Unseen(description: "the document browser shows no B1: \(app.debugDescription)")
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
