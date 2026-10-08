import CoreGraphics
import Foundation
import ScaenaKit
import Testing

/// The torture deck (PLAN 0.2): its shapes and its images.
private let torture = repository.appending(path: "tests/fixtures/torture.scaena")

/// `ops` made on the deck, as a handle let go makes them: whether they changed it.
private func made(_ session: ScaenaSession, _ ops: [JSONValue]) throws -> Bool {
    try session.tool("deck_patch", ["ops": .array(ops)]).edited
}

/// A shape's handles (PLAN 3.16, as the browser's canvas, PLAN 2.68): a rect's corner rounded to
/// the theme's radius step its drag reaches, and a polygon's points moved, added, and taken away,
/// never below the three it keeps; each one `choose`.
@Test func aShapesCornerAndPointsAreEachOneChoose() throws {
    let session = try ScaenaSession(directory: torture)
    let panel = try #require(try session.outline(state: "shapes", node: "shape-panel"))
    #expect(panel.kind == "rect" && panel.radius == 16 && panel.radii == [0, 4, 8, 16, 32, 159])
    #expect(panel.rect == CGRect(x: 96, y: 210, width: 560, height: 318))
    // The handle stands in from the top left corner as far as the radius; dragged 16 along the
    // diagonal, the radius goes as far again, to the step of 32.
    let corner = panel.corner(clear: 12)
    #expect(corner == CGPoint(x: 112, y: 226))
    let step = panel.step(from: corner, to: CGPoint(x: 128, y: 242))
    #expect(step == 4 && panel.rounds(to: step) && !panel.rounds(to: 3))
    #expect(panel.step(from: corner, to: CGPoint(x: 92, y: 206)) == 0, "dragged out past the corner: square")
    #expect(try made(session, Handling.rounding("shape-panel", to: step, in: "shapes")))
    #expect(try session.outline(state: "shapes", node: "shape-panel")?.radius == 32)

    let tri = try #require(try session.outline(state: "shapes", node: "shape-tri"))
    #expect(tri.kind == "polygon" && tri.fewest == 3 && tri.edges == 3)
    #expect(tri.points == [CGPoint(x: 0.5, y: 0), CGPoint(x: 1, y: 1), CGPoint(x: 0, y: 1)])
    #expect(tri.drawn.first == CGPoint(x: 230, y: 780))
    #expect(tri.middles.first == CGPoint(x: 297, y: 882), "the middle of the first edge, where a press adds a point")
    #expect(tri.fraction(at: CGPoint(x: 230, y: 831)) == CGPoint(x: 0.5, y: 0.25))
    #expect(tri.fraction(at: CGPoint(x: 0, y: 2000)) == CGPoint(x: 0, y: 1), "kept to the box")
    #expect(tri.removing(0) == nil, "a polygon keeps three points")
    let apex = tri.moving(0, to: tri.fraction(at: CGPoint(x: 96, y: 780)))
    #expect(try made(session, Handling.reshaping("shape-tri", to: apex, in: "shapes")))
    let moved = try #require(try session.outline(state: "shapes", node: "shape-tri"))
    #expect(moved.points == [CGPoint(x: 0, y: 0), CGPoint(x: 1, y: 1), CGPoint(x: 0, y: 1)])
    let added = moved.adding(after: 0)
    #expect(added == [CGPoint(x: 0, y: 0), CGPoint(x: 0.5, y: 0.5), CGPoint(x: 1, y: 1), CGPoint(x: 0, y: 1)])
    #expect(try made(session, Handling.reshaping("shape-tri", to: added, in: "shapes")))
    let four = try #require(try session.outline(state: "shapes", node: "shape-tri"))
    #expect(four.points.count == 4 && four.removing(1) == moved.points)
    #expect(try session.outline(state: "shapes", node: "nobody") == nil)
}

/// The rotate handle (PLAN 3.16, as the browser's canvas, PLAN 2.51): above the box, it turns the
/// node about its anchor as the pointer goes round, past a half turn and on, in whole degrees or,
/// with Shift, in fifteens; let go, one `choose` of `transform/rotate`.
@Test func theRotateHandleTurnsANodeAboutItsAnchor() throws {
    let session = try ScaenaSession(directory: torture)
    let box = try #require(try session.boxes(state: "shapes").first { $0.node == "shape-panel" })
    #expect(try session.turned(state: "shapes", node: "shape-panel") == Turned())
    let handle = try #require(box.turnHandle(arm: 24))
    #expect(handle.top == CGPoint(x: 376, y: 210) && handle.at == CGPoint(x: 376, y: 186))

    var turn = Turn(box, turned: Turned(), from: handle.at)
    #expect(turn.pivot == CGPoint(x: 376, y: 369) && turn.now == 0)
    turn.move(to: CGPoint(x: 559, y: 369), snap: false)
    #expect(turn.now == 90, "a quarter turn clockwise")
    turn.move(to: CGPoint(x: 376, y: 552), snap: false)
    turn.move(to: CGPoint(x: 193, y: 369), snap: false)
    #expect(turn.now == 270 && turn.turning == 270, "past a half turn it keeps going")

    var snapped = Turn(box, turned: Turned(), from: handle.at)
    let angle = -53 * Double.pi / 180
    let at = CGPoint(x: 376 + 100 * cos(angle), y: 369 + 100 * sin(angle))
    snapped.move(to: at, snap: false)
    #expect(snapped.now == 37)
    snapped.move(to: at, snap: true)
    #expect(snapped.now == 30, "with Shift, in fifteens")

    #expect(try made(session, Handling.turning("shape-panel", to: 15, in: "shapes")))
    #expect(try session.turned(state: "shapes", node: "shape-panel") == Turned(rotate: 15))
    let turned = try #require(try session.boxes(state: "shapes").first { $0.node == "shape-panel" })
    #expect(turned.transform != nil, "drawn turned")
}

/// An image's handles (PLAN 3.16, as the browser's canvas, PLAN 2.45, 2.74): a crop handle inside
/// each side of the part that shows crops the image from that side, a twentieth of it kept at
/// least; the focal point's handle stands at the point of the box it lines up with, and Pick reads
/// the point of the image under a press. Each is one `choose` of `crop` or `focal`, and a crop
/// that keeps the whole image is none.
@Test func anImagesCropAndFocalPointAreEachOneChoose() throws {
    let session = try ScaenaSession(directory: torture)
    let picked = try #require(try session.focal(state: "images", node: "image-focal", at: CGPoint(x: 741, y: 426)))
    #expect(abs(picked.x - 0.24) < 1e-6 && abs(picked.y - 0.5) < 1e-6, "the point of the image under the press")
    #expect(try session.focal(state: "images", node: "image-focal", at: CGPoint(x: 10, y: 10)) == nil)

    let framing = try #require(try session.framing(state: "images", node: "image-focal"))
    #expect(framing.crop == [0, 0, 1, 1] && framing.fit == "cover" && framing.size == CGSize(width: 480, height: 240))
    #expect(framing.whole == CGRect(x: 534, y: 210, width: 864, height: 432))
    #expect(framing.shown == CGRect(x: 534, y: 210, width: 414, height: 432))
    let left = framing.handle(.left, inset: 10)
    #expect(left == CGPoint(x: 544, y: 426))
    // A tenth of the whole image in from the left.
    let crop = framing.cropped(.left, from: left, to: CGPoint(x: left.x + 86.4, y: left.y))
    #expect(crop == [0.1, 0, 0.9, 1] && framing.changes(crop: crop))
    #expect(framing.cropped(.right, by: CGVector(dx: -0.01, dy: 0)) == [0, 0, 0.99, 1])
    #expect(framing.cropped(.top, by: CGVector(dx: 0, dy: 2)) == [0, 0.95, 1, 0.05], "a twentieth kept")
    #expect(Framing.keepsWhole([0, 0, 1, 1]) && !framing.changes(crop: [0, 0, 1, 1.0002]))
    #expect(try made(session, Handling.cropping("image-focal", to: crop, in: "images")))
    let cropped = try #require(try session.framing(state: "images", node: "image-focal"))
    #expect(zip(cropped.crop, [0.1, 0, 0.9, 1]).allSatisfy { abs($0 - $1) < 1e-6 } && cropped.crop.count == 4)

    #expect(framing.focalHandle == CGPoint(x: 534, y: 426) && framing.focal == CGPoint(x: 0, y: 0.5))
    #expect(!framing.changes(focal: framing.focal(at: framing.focalHandle)))
    let focal = framing.focal(at: CGPoint(x: 637.5, y: 426))
    #expect(focal == CGPoint(x: 0.25, y: 0.5) && framing.changes(focal: focal))
    #expect(try made(session, Handling.focusing("image-focal", on: focal, in: "images")))
    #expect(try session.framing(state: "images", node: "image-focal")?.focal == CGPoint(x: 0.25, y: 0.5))
    #expect(try made(session, Handling.cropping("image-focal", to: [0, 0, 1, 1], in: "images")))
    #expect(try session.framing(state: "images", node: "image-focal")?.crop == [0, 0, 1, 1], "the whole image: no crop")
}

/// The views (PLAN 3.16): the theme's grid in the format shown, the part of the canvas the surface
/// paints (none, the whole), a format beside the canvas, and find and replace on B1, every match
/// where its words live, or one.
@Test func theGridTheViewAFormatAndFindReadAsTheBrowsersDo() throws {
    let session = try ScaenaSession(directory: torture)
    let grid = try session.grid()
    #expect(grid.canvas == CGSize(width: 1920, height: 1080) && grid.columns.count == 12 && grid.rows.count == 8)
    let column = try #require(grid.columns.first)
    #expect(column.lowerBound == 96 && column.upperBound == 218 && grid.baselines.count == 112)

    try session.setView(CGRect(x: 0, y: 0, width: 960, height: 540))
    try session.setView(nil)
    #expect(throws: ScaenaError.self) { try session.setView(CGRect(x: 0, y: 0, width: 0, height: 10)) }

    let tall = try session.pixels(in: "9:16", state: "shapes", height: 320)
    #expect(tall.width == 180 && tall.height == 320 && tall.rgba.count == 180 * 320 * 4)
    let own = try session.pixels(in: nil, state: "shapes", height: 320)
    #expect(own.width == 569 && own.height == 320, "the deck's own canvas")
    #expect(throws: ScaenaError.self) { try session.pixels(in: "4:3", state: "shapes", height: 320) }

    let deck = try ScaenaSession(directory: b1)
    let found = try deck.find("Scaena")
    #expect(found.count == 4 && found.contains { $0.node == "title" })
    let claim = try #require(found.first)
    #expect(claim.kind == "claim" && claim.beat == "opening" && claim.state == "cover" && claim.matches == [[0, 6]])
    let one = try deck.replacing("Scaena", with: "Stage", one: (text: 0, match: 0))
    #expect(one.count == 1 && one.first?["path"]?.string == "/spine/sections/0/beats/0/claim")
    #expect(try made(deck, try deck.replacing("Scaena", with: "Stage")))
    #expect(try deck.find("Scaena").isEmpty && !deck.find("Stage", matchCase: true).isEmpty)
}
