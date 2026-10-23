import XCTest

/// A slide's background on the iPad (PLAN 3.32), on B1's cover: with nothing selected, the Format
/// tab offers None and each of the theme's presets, each the slide drawn small with it. A tap gives
/// the cover the soft mesh, one step that the toolbar's Undo takes back by touch. B1 is left as it
/// was.
final class BackgroundsTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testABackgroundIsChosenAndUndone() throws {
        let app = launched()
        try openB1(in: app)
        let identified = { (id: String) in
            app.descendants(matching: .any).matching(NSPredicate(format: "identifier == %@", id)).firstMatch
        }
        let tab = { (title: String) in
            app.segmentedControls.buttons.matching(NSPredicate(format: "label == %@", title)).firstMatch
        }
        let undo = app.navigationBars.buttons.matching(NSPredicate(format: "identifier == 'undo'")).firstMatch
        XCTAssertTrue(undo.waitForExistence(timeout: 5), "no Undo in the toolbar: \(app.debugDescription)")
        settle()

        let mesh = identified("background-mesh-soft")
        try XCTContext.runActivity(named: "The Format tab offers the cover's backgrounds") { _ in
            if !tab("Format").waitForExistence(timeout: 5) {
                // The inspector put away: the toolbar's Format shows it at that tab.
                app.buttons.matching(NSPredicate(format: "label == 'Format'")).firstMatch.tap()
            }
            if tab("Format").waitForExistence(timeout: 10) { tab("Format").tap() }
            guard mesh.waitForExistence(timeout: 20), identified("background-none").exists else {
                keep(app, as: "no-backgrounds")
                throw Unseen(description: "the Format tab offers no backgrounds: \(app.debugDescription)")
            }
            XCTAssertEqual(mesh.label, "Mesh Soft background")
            XCTAssertFalse(mesh.isSelected, "the cover has a background already")
            keep(app, as: "backgrounds")
        }

        try XCTContext.runActivity(named: "A tap gives the cover the mesh, and Undo takes it back") { _ in
            mesh.tap()
            guard wait(10, { mesh.isSelected }) else {
                keep(app, as: "background-not-chosen")
                throw Unseen(description: "the mesh is not the cover's background: \(app.debugDescription)")
            }
            keep(app, as: "background-chosen")
            undo.tap()
            guard wait(10, { !mesh.isSelected && identified("background-none").isSelected }) else {
                keep(app, as: "background-not-undone")
                throw Unseen(description: "Undo left the mesh: \(app.debugDescription)")
            }
        }
    }
}
