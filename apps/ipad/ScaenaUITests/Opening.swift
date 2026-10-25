import XCTest

/// What a test looked for and did not find, with what the app showed: thrown, it fails the test.
struct Unseen: Error, CustomStringConvertible {
    let description: String
}

/// What every test of the app does first: launch it, open B1, and keep what it shows.
extension XCTestCase {
    /// The app launched, the iPad held landscape, as a deck is edited, with `environment` among
    /// its environment's variables. B1 is put back as it shipped as the app starts (`-b1AsShipped`),
    /// so this test sees none of what the tests before it did to B1, saved or not.
    @MainActor
    func launched(environment: [String: String] = [:], arguments: [String] = []) -> XCUIApplication {
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication()
        app.launchEnvironment.merge(environment) { _, given in given }
        app.launchArguments += ["-b1AsShipped", "YES"] + arguments
        app.launch()
        return app
    }

    /// B1, which `apps/ipad/app-test.sh` puts in the app's own folder, open in its window, as
    /// someone opens it: in the document browser, Browse, On My iPad, the app's folder, then B1.
    /// Its first state, `cover`, is shown, found by its id in the slides down the side, with
    /// neither the browser's launch scene nor B1's cell left to press: the cover listed behind
    /// them is not open. A window the app brought back already showing it is taken as it is.
    ///
    /// A tap on B1 has gone unanswered: the browser still listing B1, or its launch scene still
    /// over the window, a minute on. So B1 is tapped again where its window has not shown in 20
    /// seconds, three taps in all, the last given a minute, each said in the log. On the first
    /// launch after the app is installed, a tap has gone unanswered for minutes and taps again
    /// changed nothing: there the app is launched again, once, and B1 tapped in it. What the app
    /// says of its opening (the GPU, the deck read, the canvas's surface) is kept in `log.txt`
    /// beside the screenshots.
    @MainActor
    func openB1(in app: XCUIApplication) throws {
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'state-cover'")).firstMatch
        // Open once the browser's launch scene and B1's cell are no longer there to be pressed.
        let prompt = app.buttons.matching(NSPredicate(format: "label == 'New from a Prompt…'")).firstMatch
        let b1 = cell(in: app, named: "b1")
        let open = { first.exists && !(prompt.exists && prompt.isHittable) && !(b1.exists && b1.isHittable) }
        if first.waitForExistence(timeout: 5), wait(5, open) { return }
        for launch in 1...2 {
            if launch > 1 {
                keep(app, as: "b1-unanswered")
                print("B1 unanswered after three taps: the app launched again")
                app.terminate()
                app.launch()
                if first.waitForExistence(timeout: 5), wait(5, open) { return }
            }
            for tap in 1...3 {
                let named = "B1 tapped in the document browser, launch \(launch), tap \(tap)"
                let tapped = XCTContext.runActivity(named: named) { _ in tapB1(in: app) }
                if !tapped { print("B1 not found in the document browser: launch \(launch), tap \(tap)") }
                if first.waitForExistence(timeout: tap == 3 ? 60 : 20), wait(5, open) { return }
                keep(app, as: "b1-still-browsing-\(launch)-\(tap)")
                let next = tap < 3 ? "tapped again" : "given up on"
                print("B1's window not shown after launch \(launch), tap \(tap): \(next)")
            }
        }
        keep(app, as: "b1-not-shown")
        keepElements(app, as: "b1-not-shown")
        throw Unseen(description: "B1's window did not show: \(app.debugDescription)")
    }

    /// The document browser walked to B1, a step at a time from wherever it stands, each tap one
    /// place further, and B1 tapped: whether it was found to tap.
    @MainActor
    private func tapB1(in app: XCUIApplication) -> Bool {
        let b1 = cell(in: app, named: "b1")
        let folder = cell(in: app, named: "Scaena")
        let onIPad = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'On My iPad'")).firstMatch
        let browse = app.buttons.matching(NSPredicate(format: "label == 'Browse'")).firstMatch
        for _ in 0..<8 {
            if b1.waitForExistence(timeout: 3) {
                b1.tap()
                return true
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
        return false
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

    /// A screenshot of the screen `app` shows, kept with the run's results as `name`, once the app
    /// is idle: the whole screen, its orientation kept with it, so it reads upright as the iPad is
    /// held. The app's own screenshot, held landscape, cut the screen's portrait buffer to a
    /// landscape frame: a quarter of the window gone, and as much of the picture black. It is still
    /// taken, for the wait it makes for the app to settle, which the screen's does not make: a test
    /// goes on from a shot as from an app at rest, the light table's slides done moving in before a
    /// drag takes one.
    @MainActor
    func keep(_ app: XCUIApplication, as name: String) {
        _ = app.screenshot()
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    /// What `app` shows as XCUITest reads it, each element with its kind, frame, identifier, and
    /// label, kept with the run's results as `name`: beside a screenshot, what each control on it
    /// is, as the Mac's walk keeps `window.txt`.
    @MainActor
    func keepElements(_ app: XCUIApplication, as name: String) {
        let elements = XCTAttachment(string: app.debugDescription)
        elements.name = name
        elements.lifetime = .keepAlways
        add(elements)
    }
}

/// The edit menu a long press opens, paged as a person pages it: shared by the tests that look
/// for an item past its first page.
extension XCTestCase {
    /// Whether `item` comes to show in the edit menu a long press opened, the menu paged on as a
    /// person pages it. The menu sets its items in one row, a page at a time, and lists none of a
    /// page it does not show; the arrow to the next page stands just past the row's last item.
    @MainActor
    func paged(to item: XCUIElement, in app: XCUIApplication) -> Bool {
        // What the menu lists: buttons, or menu items, by the system's version.
        let kinds = NSPredicate(
            format: "elementType == %lu OR elementType == %lu",
            XCUIElement.ElementType.button.rawValue, XCUIElement.ElementType.menuItem.rawValue)
        let listed = app.descendants(matching: .any).matching(kinds)
        guard wait(5, { listed.matching(NSPredicate(format: "label == 'Copy'")).firstMatch.isHittable }) else {
            return false
        }
        let row = listed.matching(NSPredicate(format: "label == 'Copy'")).firstMatch.frame.midY
        for _ in 0..<8 {
            if item.waitForExistence(timeout: 1), item.isHittable { return true }
            let shown = listed.allElementsBoundByIndex.filter { abs($0.frame.midY - row) < 6 && $0.frame.width > 0 }
            guard let last = shown.max(by: { $0.frame.maxX < $1.frame.maxX }) else { return false }
            // The arrow, where it is listed as an element of its own; else the point just past the
            // last item, where it stands.
            let said = last.label.lowercased()
            let arrow = !said.contains(where: \.isLetter) || ["forward", "next", "more"].contains { said.contains($0) }
            let x = arrow ? last.frame.midX : last.frame.maxX + 20
            app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: row)).tap()
            settle(0.5)
        }
        return item.exists && item.isHittable
    }
}
