import XCTest

/// The canvas by touch (PLAN 4.3), on B1's cover, a gesture at a time and in order, each the
/// patch the Mac's pointer makes for it: a tap selects the title, a pinch zooms and another takes
/// the whole canvas back, a double tap types in it and the keys type there (PLAN 4.4), a drag
/// moves it, and a long press offers what is done to it, whose Duplicate copies it; then, in the
/// light table, the slides down the side folded away and the toolbar where it was (PLAN 3.14), a
/// slide dragged onto another moves after it, the inspector showing the slide, not what the canvas
/// had selected, and the table put away gives the side back. The move and the copy are each one
/// step that the toolbar's Undo takes back by touch, and a long press on Undo offers Redo, named
/// for the step it makes again as the Edit menu names it, which makes the copy again (PLAN 4.11).
/// With the pen up, a pinch still zooms, and draws nothing (PLAN 2.114).
final class TouchTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testTheCanvasByTouch() throws {
        let app = launched()
        try openB1(in: app)
        // The canvas reads as the slide it shows; a test finds it by its id.
        let canvas = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        XCTAssertEqual(canvas.label, "Slide 1", "the canvas reads as no slide")
        // The title reads "Scaena", and once typed in, more.
        let titles = canvas.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'Scaena'"))
        let title = titles.firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 10), "no title on the canvas: \(app.debugDescription)")
        // Undo in the toolbar, by touch (PLAN 4.11); a test finds it by its id, apart from the
        // software keyboard's own.
        let undo = app.navigationBars.buttons.matching(NSPredicate(format: "identifier == 'undo'")).firstMatch
        XCTAssertTrue(undo.waitForExistence(timeout: 5), "no Undo in the toolbar: \(app.debugDescription)")
        settle()

        XCTContext.runActivity(named: "A tap selects the title") { _ in
            title.tap()
            XCTAssertTrue(wait(5) { title.isSelected }, "the title is not selected: \(app.debugDescription)")
            // Where the inspector shows the title's turn, it shows it as a number, 0, as a
            // presentation app does, not as a hint in an empty field (PLAN 3.28).
            let rotation = app.textFields.matching(identifier: "inspector-transform/rotate").firstMatch
            if rotation.waitForExistence(timeout: 5) {
                XCTAssertTrue(
                    wait(5) { rotation.value as? String == "0" },
                    "the title's rotation reads \(rotation.value ?? "nothing"), not 0")
            }
        }

        try XCTContext.runActivity(named: "A pinch zooms, and another shows the whole canvas") { _ in
            let before = title.frame
            canvas.pinch(withScale: 2, velocity: 1)
            guard wait(5, { shown(canvas) != "whole" }) else {
                throw Unseen(description: "the pinch zoomed nothing: \(before) → \(title.frame), \(shown(canvas))")
            }
            keep(app, as: "pinched")
            // Two fingers are no long press: nothing is offered.
            let offered = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Duplicate'")).firstMatch
            XCTAssertFalse(offered.exists, "a pinch offered a menu: \(app.debugDescription)")
            let closer = shown(canvas)
            canvas.pinch(withScale: 0.4, velocity: -1)
            let whole = wait(5) { shown(canvas) == "whole" && abs(title.frame.width - before.width) < 2 }
            keep(app, as: "unpinched")
            guard whole else {
                throw Unseen(
                    description:
                        "the canvas is not whole again: \(before) → \(title.frame), \(closer) → \(shown(canvas)), the canvas at \(canvas.frame)"
                )
            }
        }

        try XCTContext.runActivity(named: "With the pen up, a pinch zooms and draws nothing") { _ in
            // The pen taken up from the toolbar's Shape menu (PLAN 2.114): two fingers still zoom
            // and pan, as without it, and leave no stroke behind.
            let shape = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'insert-Shape'")).firstMatch
            guard shape.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no Shape in the toolbar: \(app.debugDescription)")
            }
            shape.tap()
            let draw = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Draw with Pen'")).firstMatch
            guard draw.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the Shape menu offers no pen: \(app.debugDescription)")
            }
            draw.tap()
            settle()
            canvas.pinch(withScale: 2, velocity: 1)
            guard wait(5, { shown(canvas) != "whole" }) else {
                throw Unseen(description: "with the pen up, the pinch zoomed nothing: \(shown(canvas))")
            }
            settle()
            keep(app, as: "pen-pinched")
            let drawing = canvas.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'Drawing'"))
            XCTAssertEqual(drawing.count, 0, "the pinch drew a stroke: \(app.debugDescription)")
            canvas.pinch(withScale: 0.4, velocity: -1)
            XCTAssertTrue(wait(5) { shown(canvas) == "whole" }, "the canvas is not whole again: \(shown(canvas))")
            shape.tap()
            let stop = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Stop Drawing'")).firstMatch
            guard stop.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the Shape menu offers no way to put the pen down: \(app.debugDescription)")
            }
            stop.tap()
            settle()
        }

        try XCTContext.runActivity(named: "A double tap types in the title, and the keys type there") { _ in
            title.doubleTap()
            let typing = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'typing'")).firstMatch
            guard typing.waitForExistence(timeout: 5), typing.label == "Editing text" else {
                throw Unseen(description: "a double tap typed in nothing: \(app.debugDescription)")
            }
            // The keyboard's keys, through `UITextInput`, at the caret the double tap put there.
            app.typeText("!")
            let typed = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena!'")).firstMatch
            guard typed.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the keys typed nothing in the title: \(app.debugDescription)")
            }
            keep(app, as: "typing")
            // A tap where nothing draws stops typing.
            canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.03, dy: 0.97)).tap()
            XCTAssertTrue(wait(5) { !typing.exists }, "still typing: \(app.debugDescription)")
        }

        try XCTContext.runActivity(named: "A drag moves the title, and Undo takes the move back") { _ in
            title.tap()
            let before = title.frame
            let from = title.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            from.press(forDuration: 0.1, thenDragTo: from.withOffset(CGVector(dx: 0, dy: 160)))
            guard wait(5, { abs(title.frame.midY - before.midY) > 20 }) else {
                throw Unseen(description: "the drag moved nothing: \(before) → \(title.frame)")
            }
            keep(app, as: "dragged")
            undo.tap()
            guard wait(5, { abs(title.frame.midY - before.midY) < 2 }) else {
                throw Unseen(description: "Undo did not take the move back: \(before) → \(title.frame)")
            }
        }

        try XCTContext.runActivity(named: "A long press offers what is done to the title, Duplicate copies it, and Undo takes the copy away") { _ in
            title.press(forDuration: 1.2)
            let duplicate = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Duplicate'")).firstMatch
            guard duplicate.waitForExistence(timeout: 5) else {
                throw Unseen(description: "a long press offered nothing: \(app.debugDescription)")
            }
            keep(app, as: "offered")
            XCTAssertTrue(title.isSelected, "the title pressed is not selected")
            duplicate.tap()
            guard wait(5, { titles.count == 2 }) else {
                throw Unseen(description: "Duplicate made no copy: \(app.debugDescription)")
            }
            settle()
            keep(app, as: "duplicated")
            undo.tap()
            guard wait(5, { titles.count == 1 }) else {
                throw Unseen(description: "Undo left the copy: \(app.debugDescription)")
            }
            // A long press on Undo offers Redo, which makes the copy again.
            undo.press(forDuration: 1.2)
            let redo = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'Redo'")).firstMatch
            guard redo.waitForExistence(timeout: 5) else {
                keep(app, as: "no-redo")
                throw Unseen(description: "a long press on Undo offered no Redo: \(app.debugDescription)")
            }
            keep(app, as: "redo-offered")
            XCTAssertEqual(redo.label, "Redo Duplicate", "Redo does not name the step it makes again")
            redo.tap()
            guard wait(5, { titles.count == 2 }) else {
                throw Unseen(description: "Redo made no copy again: \(app.debugDescription)")
            }
        }

        try XCTContext.runActivity(named: "The Image menu offers Map…, whose sheet asks where, and Cancel puts it away") { _ in
            // PLAN O.1d: Insert › Map… from the toolbar's Image menu, as the Mac's offers it.
            let offered = app.descendants(matching: .any)
            let image = offered.matching(NSPredicate(format: "identifier == 'insert-Image'")).firstMatch
            guard image.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no Image in the toolbar: \(app.debugDescription)")
            }
            image.tap()
            let map = offered.matching(NSPredicate(format: "label == 'Map…'")).firstMatch
            guard map.waitForExistence(timeout: 5) else {
                keep(app, as: "no-map")
                throw Unseen(description: "the Image menu offers no Map…: \(app.debugDescription)")
            }
            map.tap()
            let place = offered.matching(NSPredicate(format: "identifier == 'map-where'")).firstMatch
            guard place.waitForExistence(timeout: 5) else {
                throw Unseen(description: "Map… opened no sheet asking where: \(app.debugDescription)")
            }
            keep(app, as: "map")
            app.buttons.matching(NSPredicate(format: "label == 'Cancel'")).firstMatch.tap()
            guard wait(5, { !place.exists }) else {
                throw Unseen(description: "Cancel left the map's sheet: \(app.debugDescription)")
            }
        }

        try XCTContext.runActivity(named: "In the light table, a slide dragged onto another moves after it") { _ in
            // The slides down the side, where their first stands before the light table shows.
            let cells = app.descendants(matching: .any)
            let side = cells.matching(NSPredicate(format: "identifier == 'state-cover'")).firstMatch
            guard side.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no slides down the side: \(app.debugDescription)")
            }
            let sideAt = side.frame
            // The toolbar's View menu, as a presentation app's, shows the light table.
            let view = app.buttons.matching(NSPredicate(format: "label == 'View'")).firstMatch
            guard view.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no View in the toolbar: \(app.debugDescription)")
            }
            view.tap()
            let table = cells.matching(NSPredicate(format: "label == 'Light Table'")).firstMatch
            guard table.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the View menu offers no light table: \(app.debugDescription)")
            }
            table.tap()
            // Each slide reads its number and its steps, and a test finds it by its id: B1's first
            // is the cover, its third belief-1.
            let cover = cells.matching(NSPredicate(format: "identifier == 'slide-cover'")).firstMatch
            let third = cells.matching(NSPredicate(format: "identifier == 'slide-belief-1'")).firstMatch
            guard cover.waitForExistence(timeout: 10), third.exists, cover.label.hasPrefix("Slide 1"),
                third.label.hasPrefix("Slide 3")
            else {
                throw Unseen(description: "the light table shows no cover and belief-1: \(app.debugDescription)")
            }
            // The slides down the side fold away while the light table shows every slide, as on the
            // Mac (PLAN 3.14): the table's first slide stands where the side stood, not beside it.
            // The toolbar stays put: View and Undo at its leading end, before Add Slide.
            let folded = wait(5) { cover.frame.minX < sideAt.maxX }
            keep(app, as: "slides")
            XCTAssertTrue(
                folded, "the slides down the side show beside the light table: its first at \(cover.frame), the side's at \(sideAt)")
            let add = app.buttons.matching(NSPredicate(format: "identifier == 'add-slide'")).firstMatch
            guard add.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no Add Slide in the toolbar: \(app.debugDescription)")
            }
            XCTAssertTrue(
                view.frame.maxX <= add.frame.minX && undo.frame.maxX <= add.frame.minX,
                "View and Undo left the toolbar's leading end: View at \(view.frame), Undo at \(undo.frame), Add Slide at \(add.frame)")
            // The inspector shows the slide, as a presentation app's light table does: not the
            // copy the canvas had selected, which the table does not show.
            let slideInspector = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'layout-choose'")).firstMatch
            XCTAssertTrue(slideInspector.waitForExistence(timeout: 5), "the inspector does not show the slide: \(app.debugDescription)")
            // A slide lifts once held, as any drag on the iPad does, and lands where it is let go.
            // The simulator can lift the slide and lose the drag's movement, its system gesture
            // gate timing out, so the slide drops back where it was: only then is the drag made
            // again, and the slide must still land after belief-1.
            let moved = cells.matching(NSPredicate(format: "identifier == 'slide-cover' AND label BEGINSWITH 'Slide 3'")).firstMatch
            for attempt in 1...2 where !moved.exists && cover.label.hasPrefix("Slide 1") {
                cover.press(forDuration: 1.5, thenDragTo: third, withVelocity: .slow, thenHoldForDuration: 1)
                if !moved.waitForExistence(timeout: 5), attempt == 1 {
                    keep(app, as: "slide-dropped-back")
                }
            }
            guard moved.exists else {
                throw Unseen(description: "the cover did not move after belief-1: \(app.debugDescription)")
            }
            keep(app, as: "slide-moved")
            // Put away, the light table gives the slides down the side back where they stood.
            view.tap()
            guard table.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the View menu offers no light table to put away: \(app.debugDescription)")
            }
            table.tap()
            let back = wait(10) { side.exists && side.isHittable && abs(side.frame.minX - sideAt.minX) < 2 }
            keep(app, as: "slides-put-away")
            XCTAssertTrue(back, "the slides down the side did not come back: \(app.debugDescription)")
        }
    }

    /// Several selected by touch alone (lift-up FUNC-4), on B1's cover: with the toolbar's Select
    /// held, a tap adds the subtitle to the title selected, and a tap on the title takes it out, as
    /// Shift and a click do; let go, a tap selects one again. A long press offers Only on This Slide,
    /// which a tap holds, shown checked at the next long press, as Option would keep a gesture to
    /// the slide. In the light table, with Select held, a tap adds a slide to those picked, as ⌘ and
    /// a click do.
    @MainActor
    func testSelectSeveralByTouch() throws {
        let app = launched()
        try openB1(in: app)
        let all = app.descendants(matching: .any)
        let canvas = all.matching(NSPredicate(format: "identifier == 'canvas'")).firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        let title = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'")).firstMatch
        let subtitle = canvas.descendants(matching: .any)
            .matching(NSPredicate(format: "label BEGINSWITH 'Why this exists'")).firstMatch
        guard title.waitForExistence(timeout: 10), subtitle.exists else {
            throw Unseen(description: "the cover reads no title and subtitle: \(app.debugDescription)")
        }
        // Select in the toolbar, beside Undo; a test finds it by its id.
        let select = app.navigationBars.buttons.matching(NSPredicate(format: "identifier == 'select-several'")).firstMatch
        guard select.waitForExistence(timeout: 5) else {
            throw Unseen(description: "no Select in the toolbar: \(app.debugDescription)")
        }
        settle()

        try XCTContext.runActivity(named: "With Select held, a tap adds to the selection, and another takes from it") { _ in
            title.tap()
            guard wait(5, { title.isSelected && !subtitle.isSelected }) else {
                throw Unseen(description: "a tap did not select the title alone: \(app.debugDescription)")
            }
            select.tap()
            subtitle.tap()
            guard wait(5, { title.isSelected && subtitle.isSelected }) else {
                keep(app, as: "not-two-selected")
                throw Unseen(description: "with Select held, a tap on the subtitle did not add it: \(app.debugDescription)")
            }
            keep(app, as: "two-selected")
            title.tap()
            guard wait(5, { !title.isSelected && subtitle.isSelected }) else {
                throw Unseen(description: "with Select held, a tap on the title did not take it out: \(app.debugDescription)")
            }
            // Let go, a tap selects one alone again.
            select.tap()
            title.tap()
            guard wait(5, { title.isSelected && !subtitle.isSelected }) else {
                throw Unseen(description: "with Select let go, a tap did not select the title alone: \(app.debugDescription)")
            }
        }

        try XCTContext.runActivity(named: "A long press offers Only on This Slide, which a tap holds") { _ in
            title.press(forDuration: 1.2)
            let only = all.matching(NSPredicate(format: "label == 'Only on This Slide'")).firstMatch
            guard paged(to: only, in: app) else {
                keep(app, as: "no-only-this-slide")
                throw Unseen(description: "a long press offered no Only on This Slide: \(app.debugDescription)")
            }
            XCTAssertFalse(only.isSelected, "Only on This Slide is shown held before it was asked for")
            only.tap()
            settle(1)
            title.press(forDuration: 1.2)
            guard paged(to: only, in: app) else {
                throw Unseen(description: "a second long press offered no Only on This Slide: \(app.debugDescription)")
            }
            keep(app, as: "only-this-slide")
            XCTAssertTrue(only.isSelected, "Only on This Slide is not shown held once asked for")
            // Let it go.
            only.tap()
            settle(1)
        }

        try XCTContext.runActivity(named: "In the light table, with Select held, a tap adds a slide") { _ in
            let view = app.buttons.matching(NSPredicate(format: "label == 'View'")).firstMatch
            guard view.waitForExistence(timeout: 5) else {
                throw Unseen(description: "no View in the toolbar: \(app.debugDescription)")
            }
            view.tap()
            let table = all.matching(NSPredicate(format: "label == 'Light Table'")).firstMatch
            guard table.waitForExistence(timeout: 5) else {
                throw Unseen(description: "the View menu offers no light table: \(app.debugDescription)")
            }
            table.tap()
            let cover = all.matching(NSPredicate(format: "identifier == 'slide-cover'")).firstMatch
            let third = all.matching(NSPredicate(format: "identifier == 'slide-belief-1'")).firstMatch
            guard cover.waitForExistence(timeout: 10), third.exists else {
                throw Unseen(description: "the light table shows no cover and belief-1: \(app.debugDescription)")
            }
            cover.tap()
            guard wait(5, { cover.isSelected && !third.isSelected }) else {
                throw Unseen(description: "a tap did not pick the cover alone: \(app.debugDescription)")
            }
            select.tap()
            third.tap()
            guard wait(5, { cover.isSelected && third.isSelected }) else {
                keep(app, as: "not-two-slides")
                throw Unseen(description: "with Select held, a tap did not add belief-1: \(app.debugDescription)")
            }
            keep(app, as: "two-slides")
            // Let go, a tap picks one alone again.
            select.tap()
            third.tap()
            guard wait(5, { !cover.isSelected && third.isSelected }) else {
                throw Unseen(description: "with Select let go, a tap did not pick belief-1 alone: \(app.debugDescription)")
            }
        }
    }

    /// How close the canvas is shown, as it tells VoiceOver: "whole" where nothing is zoomed.
    @MainActor
    private func shown(_ canvas: XCUIElement) -> String {
        let value = canvas.value as? String ?? ""
        return value.isEmpty ? "whole" : value
    }
}
