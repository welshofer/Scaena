#if os(macOS)
import AppKit
#endif
import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// The torture deck (PLAN 0.2): laid out in 9:16 too, some nodes with layouts of their own there.
private let torture = repository.appending(path: "tests/fixtures/torture.scaena")

/// Whether two rects are the same to a millionth of a point or a unit.
private func alike(_ a: CGRect, _ b: CGRect) -> Bool {
    abs(a.minX - b.minX) < 1e-6 && abs(a.minY - b.minY) < 1e-6 && abs(a.width - b.width) < 1e-6
        && abs(a.height - b.height) < 1e-6
}

/// Zoom (PLAN 3.16): the slide grows on the desk, as a presentation app's does, not what is on it
/// within the slide's place. Fitted at first, in the middle of the desk, it goes a step closer about
/// the middle of what shows, as close as asked about a point that stays put, from fitted to eight
/// times as large, panned no farther than the margin past its edges. What the desk shows is the
/// canvas's view, which the session paints through; the whole slide where it all shows.
@Test func theSlideGrowsOnTheDeskAsItIsZoomed() {
    // A desk the slide fits on at half its size, the margin about it.
    var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 596))
    #expect(zoom.level == 1 && zoom.view == nil && !zoom.pans)
    #expect(alike(zoom.slide, CGRect(x: 28, y: 28, width: 960, height: 540)), "fitted, in the middle: \(zoom.slide)")
    let still = zoom.pan(by: CGVector(dx: 100, dy: 0))
    #expect(!still, "nothing to pan where the whole slide shows")
    zoom.step(1)
    #expect(zoom.level == 1.5 && alike(zoom.slide, CGRect(x: -212, y: -107, width: 1440, height: 810)), "the slide grows")
    #expect(alike(zoom.visible, CGRect(x: 0, y: 0, width: 1016, height: 596)), "onto the desk about it: \(zoom.visible)")
    zoom.zoom(to: 2)
    #expect(alike(zoom.shown, CGRect(x: 452, y: 242, width: 1016, height: 596)), "about the middle: \(zoom.shown)")
    #expect(zoom.view.map { alike($0, zoom.shown) } == true, "painted through what shows")
    zoom.zoom(to: 4, about: .zero)
    #expect(alike(zoom.shown, CGRect(x: 452, y: 242, width: 508, height: 298)), "the point asked about stays put")
    let moved = zoom.pan(by: CGVector(dx: 10_000, dy: 0))
    #expect(moved)
    #expect(abs(zoom.slide.maxX - (1016 - Zoom.margin)) < 1e-6, "kept on the desk, its edge the margin in")
    #expect(alike(zoom.visible, CGRect(x: 0, y: 0, width: 988, height: 596)) && abs(zoom.shown.maxX - 1920) < 1e-6)
    zoom.step(-1)
    #expect(zoom.level == 3 && abs(zoom.shown.maxX - 1920) < 1e-6)
    zoom.zoom(to: 100)
    #expect(zoom.level == 8, "eight times at most")
    zoom.zoom(to: 0.5)
    #expect(zoom.level == 1 && zoom.view == nil, "fitted at least")
    #expect(alike(zoom.slide, CGRect(x: 28, y: 28, width: 960, height: 540)), "in the middle again")
    zoom.step(1)
    zoom.resize(to: CGSize(width: 1920, height: 1080))
    #expect(zoom.level == 1.5, "the same canvas changes nothing")
    zoom.resize(to: CGSize(width: 1080, height: 1920))
    #expect(zoom.level == 1 && zoom.view == nil, "another canvas is fitted")
    #expect(alike(zoom.slide, CGRect(x: 356.125, y: 28, width: 303.75, height: 540)), "\(zoom.slide)")

    // On a desk taller than the slide is wide, it grows down the desk to fill it before it pans
    // that way: what fits shows whole, in the middle.
    var tall = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 1500))
    #expect(alike(tall.slide, CGRect(x: 28, y: 480, width: 960, height: 540)))
    tall.zoom(to: 2)
    #expect(alike(tall.visible, CGRect(x: 0, y: 210, width: 1016, height: 1080)), "the slide's whole height shows")
    #expect(alike(tall.shown, CGRect(x: 452, y: 0, width: 1016, height: 1080)))
    tall.pan(by: CGVector(dx: 0, dy: 100))
    #expect(alike(tall.slide, CGRect(x: -452, y: 210, width: 1920, height: 1080)), "it stays in the middle down the desk")

    // The desk resized: as close as it was, what showed in the middle in the middle still.
    var resized = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 596))
    resized.zoom(to: 2)
    resized.place(on: CGSize(width: 1216, height: 696))
    #expect(resized.level == 2 && abs(resized.shown.midX - 960) < 1e-6 && abs(resized.shown.midY - 540) < 1e-6)
    #expect(resized.placed(on: CGSize(width: 1216, height: 696)) == resized, "the same desk changes nothing")
}

/// The arrows with nothing selected (PLAN 4.12, as the browser's preview pans, PLAN 2.125): zoomed
/// in, what shows moves a tenth of itself, or half with Shift, across or down, the slide kept on the
/// desk, so that at its edge an arrow goes no farther and is still the pan's, and the slide is as
/// close as it was; fitted, the whole slide shows, and the arrows go on.
@Test func theArrowsPanATenthOfWhatShowsOrHalfWithShift() {
    var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 596))
    // Each pan made before it is expected: a mutating call in `#expect` would be made on a copy.
    let fitted = zoom.pan(1, 0, far: false)
    #expect(!fitted, "fitted, the arrows go on")
    #expect(zoom.level == 1 && zoom.view == nil)
    zoom.zoom(to: 4)
    let middle = zoom.shown
    #expect(alike(middle, CGRect(x: 706, y: 391, width: 508, height: 298)), "\(middle)")
    let across = zoom.pan(1, 0, far: false)
    #expect(across)
    #expect(alike(zoom.shown, middle.offsetBy(dx: middle.width / 10, dy: 0)), "a tenth across: \(zoom.shown)")
    let down = zoom.pan(0, 1, far: true)
    #expect(down)
    let half = middle.offsetBy(dx: middle.width / 10, dy: middle.height / 2)
    #expect(alike(zoom.shown, half), "half down: \(zoom.shown)")
    zoom.pan(-1, 0, far: false)
    zoom.pan(0, -1, far: true)
    #expect(alike(zoom.shown, middle), "and back: \(zoom.shown)")
    #expect(zoom.level == 4, "as close as it was")
    // At the edge, no farther: the slide's edge the margin in from the desk's, and the key still
    // the pan's.
    for _ in 0..<8 { zoom.pan(1, 0, far: true) }
    let past = zoom.pan(1, 0, far: true)
    #expect(past)
    #expect(abs(zoom.shown.maxX - 1920) < 1e-6 && abs(zoom.slide.maxX - (1016 - Zoom.margin)) < 1e-6, "\(zoom.slide)")
}

/// A page turned zoomed in (PLAN 4.12): the slide turned to shows as close as the last, its middle
/// in the middle of the desk, as it is zoomed from fitted, wherever the last was panned to: a
/// corner of the last may hold nothing of the next. Fitted, the whole slide shows as before.
@Test func aPageTurnedShowsTheSlidesMiddleAsCloseAsItWas() {
    var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 596))
    zoom.center()
    #expect(zoom.level == 1 && zoom.view == nil, "fitted, the whole slide shows")
    #expect(alike(zoom.slide, CGRect(x: 28, y: 28, width: 960, height: 540)), "in the middle: \(zoom.slide)")
    zoom.zoom(to: 2)
    let middle = zoom.shown
    for _ in 0..<4 { zoom.pan(1, 1, far: true) }
    #expect(!alike(zoom.shown, middle), "panned to its corner: \(zoom.shown)")
    zoom.center()
    #expect(zoom.level == 2, "as close as it was")
    #expect(alike(zoom.shown, middle), "its middle in the middle of the desk: \(zoom.shown)")
}

/// What Tab, Return, a selection by keys, or a panel selects comes into view (PLAN 4.12, as the
/// browser's, PLAN 2.125): zoomed in, what shows moves the least that shows the box whole, and
/// nothing where it shows whole already; a box larger than what shows, the least that shows nothing
/// but it; and of a box past the canvas's edge, what the slide draws. Fitted, nothing moves. B1's
/// cover at four times fitted: its subtitle is wider than what shows, and its title larger both
/// ways. A box is taken where it is drawn: a turned one about its corners.
@Test func whatIsSelectedComesIntoViewTheLeastWay() throws {
    let boxes = try ScaenaSession(directory: b1).boxes(state: "cover")
    let title = try #require(boxes.first { $0.node == "title" })
    let subtitle = try #require(boxes.first { $0.node == "subtitle" })
    #expect(title.extent == CGRect(x: title.rect[0], y: title.rect[1], width: title.rect[2], height: title.rect[3]))
    let turned = try JSONDecoder().decode(
        NodeBox.self, from: Data(#"{"node": "a", "rect": [0, 0, 100, 50], "draws": true, "transform": [0, 1, -1, 0, 100, 0]}"#.utf8))
    #expect(turned.extent == CGRect(x: 50, y: 0, width: 50, height: 100), "about its corners: \(turned.extent)")

    var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 596))
    // Each reveal made before it is expected: a mutating call in `#expect` would be made on a copy.
    let fitted = zoom.reveal(subtitle.extent)
    #expect(!fitted, "fitted, the whole slide shows")
    zoom.zoom(to: 4)
    let middle = zoom.shown
    let whole = zoom.reveal(CGRect(x: 800, y: 450, width: 100, height: 100))
    #expect(!whole, "shown whole already")
    #expect(alike(zoom.shown, middle))
    // Out of view to the right and below: just far enough that its far corner shows.
    let far = zoom.reveal(CGRect(x: 1300, y: 700, width: 100, height: 50))
    #expect(far)
    #expect(alike(zoom.shown, CGRect(x: 892, y: 452, width: 508, height: 298)), "\(zoom.shown)")
    // To the left and above: its near corner.
    let near = zoom.reveal(CGRect(x: 200, y: 100, width: 100, height: 50))
    #expect(near)
    #expect(alike(zoom.shown, CGRect(x: 200, y: 100, width: 508, height: 298)), "\(zoom.shown)")
    // The subtitle, wider than what shows, which lies across it already: down, the least move that
    // shows it whole, its foot at the foot of what shows.
    let words = subtitle.extent
    #expect(words.width > middle.width && words.height < middle.height)
    let down = zoom.reveal(words)
    #expect(down)
    #expect(abs(zoom.shown.minX - 200) < 1e-6 && abs(zoom.shown.maxY - words.maxY) < 1e-6, "\(zoom.shown)")
    // The title, larger both ways: what shows comes to lie within it, moved the least way, up to
    // its foot; then nothing but it shows already.
    let heading = title.extent
    #expect(heading.width > middle.width && heading.height > middle.height)
    let up = zoom.reveal(heading)
    #expect(up)
    #expect(heading.contains(zoom.shown) && abs(zoom.shown.maxY - heading.maxY) < 1e-6, "\(zoom.shown)")
    let again = zoom.reveal(heading)
    #expect(!again, "nothing but it shows already")
    // Larger than what shows and out of view to the right: its near edge at the edge of what shows.
    let larger = zoom.reveal(CGRect(x: 1000, y: 300, width: 800, height: 500))
    #expect(larger)
    #expect(abs(zoom.shown.minX - 1000) < 1e-6, "\(zoom.shown)")
    // Past the canvas's edge: what the slide draws of it.
    let past = zoom.reveal(CGRect(x: 1800, y: 900, width: 400, height: 400))
    #expect(past)
    #expect(abs(zoom.shown.maxX - 1920) < 1e-6 && abs(zoom.shown.maxY - 1080) < 1e-6, "\(zoom.shown)")
    let beyond = zoom.reveal(CGRect(x: 2000, y: 0, width: 100, height: 100))
    #expect(!beyond, "wholly past it: nothing")
    #expect(zoom.level == 4, "as close as it was")
}

/// The formats side by side (PLAN 3.16, as the browser's, PLAN 2.62, 2.85): the canvas shown in
/// 9:16 and back, one the deck does not list refused; a node given a layout of its own there,
/// once; and each finding saying the formats it holds in.
@Test func theCanvasIsShownInAFormatAndANodePlacedAnewThere() async throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: torture))
    let session = editor.session
    // Every state linted, as the window lints it once it opens (PLAN 4.13).
    await editor.lintEvery()
    #expect(editor.whole)
    #expect(try session.formats() == ["9:16"])
    // Its own canvas named by its shape, as the strip names it, and the one format laid out anew.
    let sizes = try session.sizes()
    #expect(sizes.own == "16:9" && sizes.anew == ["9:16"])
    let revision = editor.revision
    try editor.show(format: "9:16")
    #expect(editor.format == "9:16" && editor.revision > revision)
    #expect(try session.canvasSize() == CGSize(width: 1080, height: 1920))
    #expect(throws: ScaenaError.self) { try editor.show(format: "4:3") }
    #expect(editor.format == "9:16", "the format shown as it was")

    let anew = try #require(try session.placingAnew("shape-panel", state: "shapes", in: "9:16"))
    #expect(anew.first?["op"]?.string == "place" && anew.first?["anew"] == true)
    #expect(anew.first?["format"]?.string == "9:16" && anew.first?["at"]?["col"] != nil)
    #expect(try session.tool("deck_patch", ["ops": .array(anew)]).edited)
    #expect(try session.placingAnew("shape-panel", state: "shapes", in: "9:16") == nil, "it has one there now")
    #expect(try session.placingAnew("x-rank", state: "across", in: "9:16") == nil, "one the deck gave it")

    try editor.show(format: nil)
    #expect(try editor.format == nil && session.canvasSize() == CGSize(width: 1920, height: 1080))
    #expect(editor.findings.contains { $0.state == "shapes" && $0.formats == ["", "9:16"] })
}

#if os(macOS)  // the canvas's keys are AppKit's; the iPad's come with PLAN 4.4 and 4.6
/// Find's matches (PLAN 3.16, as the browser's find bar, PLAN 2.47): each match of each text found,
/// in order, stepped through both ways and round; and the Edit menu's Find on the canvas, which
/// opens the deck's find bar.
@MainActor
@Test func findStepsThroughEveryMatchAndTheCanvasTakesTheFindMenu() throws {
    let session = try ScaenaSession(directory: b1)
    let found = try session.find("Scaena")
    let matches = Matches(found)
    #expect(matches.all.count == found.map(\.matches.count).reduce(0, +) && matches.all.count >= 4)
    #expect(matches.all.first?.text == 0 && matches.all.first?.match == 0)
    #expect(matches.next(after: nil) == 0 && matches.next(after: matches.all.count - 1) == 0, "round to the first")
    #expect(matches.previous(before: nil) == matches.all.count - 1 && matches.previous(before: 0) == matches.all.count - 1)
    #expect(Matches([]).next(after: nil) == nil)

    let keys = CanvasKeys(typing: Typing(editor: DeckEditor(session: session)))
    var asked: [NSTextFinder.Action] = []
    keys.finding = { asked.append($0) }
    let next = NSMenuItem(title: "Find Next", action: nil, keyEquivalent: "g")
    next.tag = NSTextFinder.Action.nextMatch.rawValue
    keys.performTextFinderAction(next)
    keys.performFindPanelAction(nil)
    #expect(asked == [.nextMatch, .showFindInterface])
}
#endif

/// The Zoom menu says how large the slide is drawn against its own size, as a presentation app's
/// does, whatever the desk: 100% a point to a canvas unit, not the slide fitted. Fitted at half its
/// size on this desk, it reads 50%, and 200% draws it twice its own size, four times fitted. It
/// offers what is closer than fitted, and no closer than eight times fitted.
@Test func theZoomSaysTheSlidesSizeAgainstItsOwn() {
    var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 1016, height: 596))
    #expect(zoom.percent == 50, "fitted at half its size: \(zoom.percent)%")
    #expect(!zoom.offers(0.25) && !zoom.offers(0.5), "no farther than fitted")
    #expect(zoom.offers(0.75) && zoom.offers(1) && zoom.offers(4))
    zoom.zoom(toActual: 2)
    #expect(zoom.level == 4 && zoom.percent == 200 && abs(zoom.slide.width - 3840) < 1e-6, "\(zoom.level)")
    zoom.step(1)
    #expect(zoom.percent == 300, "a step closer: \(zoom.percent)%")
    // A smaller desk fits it smaller: fitted, it reads less, and eight times fitted is closer to it.
    var small = Zoom(canvas: CGSize(width: 1920, height: 1080), desk: CGSize(width: 536, height: 326))
    #expect(small.percent == 25, "\(small.percent)%")
    #expect(small.offers(2) && !small.offers(3), "eight times fitted at most")
    small.zoom(toActual: 1)
    #expect(small.level == 4 && small.percent == 100)
}
