#if os(macOS)
import AppKit
#endif
import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// The torture deck (PLAN 0.2): laid out in 9:16 too, some nodes with layouts of their own there.
private let torture = repository.appending(path: "tests/fixtures/torture.scaena")

/// Zoom (PLAN 3.16, as the browser's preview, PLAN 2.46): a step closer about the middle of what
/// shows, as close as asked about a point that stays put, from the whole canvas to eight times as
/// close, kept on the canvas, and the whole canvas again, which the session paints through none.
@Test func theCanvasZoomsAndPansAsTheBrowsersPreviewDoes() {
    var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080))
    #expect(zoom.level == 1 && zoom.view == nil)
    zoom.step(1)
    #expect(zoom.level == 1.5 && zoom.shown == CGRect(x: 320, y: 180, width: 1280, height: 720), "about the middle")
    zoom.step(1)
    #expect(zoom.shown == CGRect(x: 480, y: 270, width: 960, height: 540))
    zoom.zoom(to: 4, about: CGPoint(x: 480, y: 270))
    #expect(zoom.shown == CGRect(x: 480, y: 270, width: 480, height: 270), "the point asked about stays put")
    zoom.pan(by: CGVector(dx: 10_000, dy: 0))
    #expect(zoom.shown == CGRect(x: 1440, y: 270, width: 480, height: 270), "kept on the canvas")
    zoom.step(-1)
    #expect(zoom.level == 3 && zoom.shown == CGRect(x: 1280, y: 225, width: 640, height: 360))
    zoom.look(CGRect(x: 0, y: 0, width: 100, height: 50))
    #expect(zoom.level == 8 && zoom.shown == CGRect(x: 0, y: 0, width: 240, height: 135), "eight times at most")
    zoom.look(CGRect(x: 0, y: 0, width: 5000, height: 5000))
    #expect(zoom.level == 1 && zoom.view == nil, "the whole canvas at least")
    zoom.step(1)
    zoom.resize(to: CGSize(width: 1920, height: 1080))
    #expect(zoom.level == 1.5, "the same size changes nothing")
    zoom.resize(to: CGSize(width: 1080, height: 1920))
    #expect(zoom.view == nil && zoom.shown == CGRect(x: 0, y: 0, width: 1080, height: 1920), "another size shows it whole")
}

/// The formats side by side (PLAN 3.16, as the browser's, PLAN 2.62, 2.85): the canvas shown in
/// 9:16 and back, one the deck does not list refused; a node given a layout of its own there,
/// once; and each finding saying the formats it holds in.
@Test func theCanvasIsShownInAFormatAndANodePlacedAnewThere() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: torture))
    let session = editor.session
    #expect(try session.formats() == ["9:16"])
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
    editor.lintEvery()
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
