import CoreGraphics
import Foundation

// The canvas's views (PLAN 3.16), as the browser's preview keeps them: how close the slide is and
// what part of it shows (PLAN 2.46), the format it is laid out in (PLAN 2.62, 2.85), and the
// matches a find steps through (PLAN 2.47). Nothing here lays out or paints: the session paints
// through the part shown, and lays each format out once.

/// How close the slide is shown on the desk the window gives it (PLAN 3.16): fitted to the desk at
/// first, then as much as eight times as large, about a point that stays where it is. The slide
/// grows on the desk, as a presentation app's does, and nothing on it is drawn larger than the
/// slide it is on: what of the slide fits on the desk shows, the desk about it, and the rest is a
/// pan away, kept on the desk to a margin past its edges. The canvas's view is the part of the
/// slide the desk shows, which the session paints through (PLAN 2.46).
public struct Zoom: Equatable, Sendable {
    /// The steps a step closer or farther goes by.
    public static let steps: [Double] = [1, 1.5, 2, 3, 4, 6, 8]
    /// The closest it goes.
    public static let most = 8.0
    /// The desk kept about the slide fitted, and past an edge it is panned to, points.
    public static let margin: CGFloat = 28
    /// The coordinate space a point on the desk is read in: the desk's own.
    public static let space = "desk"

    /// The canvas, canvas units.
    public private(set) var canvas: CGSize
    /// The desk the slide is shown on, points: the room the window gives the canvas.
    public private(set) var desk: CGSize
    /// How close: 1 shows the slide fitted to the desk.
    public private(set) var level: Double = 1
    /// The slide's top left corner on the desk, points.
    private var corner: CGPoint = .zero

    public init(canvas: CGSize, desk: CGSize = .zero) {
        self.canvas = canvas
        self.desk = desk
        settle()
    }

    /// How large the slide is fitted to the desk, within its margin: its size at `level` 1.
    public var fitted: CGSize {
        let (w, h) = (max(desk.width - 2 * Self.margin, 1), max(desk.height - 2 * Self.margin, 1))
        let shape = canvas.width / max(canvas.height, 1)
        return w / h > shape ? CGSize(width: h * shape, height: h) : CGSize(width: w, height: w / shape)
    }

    /// Points on the desk to a canvas unit.
    public var scale: CGFloat { fitted.width * CGFloat(level) / max(canvas.width, 1) }

    /// The slide on the desk, points: the whole of it, whether it all shows or not.
    public var slide: CGRect {
        CGRect(origin: corner, size: CGSize(width: canvas.width * scale, height: canvas.height * scale))
    }

    /// The part of the slide the desk shows, points: where the canvas's view stands on the desk.
    public var visible: CGRect {
        let part = slide.intersection(CGRect(origin: .zero, size: desk))
        return part.isNull ? .zero : part
    }

    /// The part of the canvas shown, canvas units.
    public var shown: CGRect {
        let (s, v) = (max(scale, 1e-9), visible)
        return CGRect(x: (v.minX - corner.x) / s, y: (v.minY - corner.y) / s, width: v.width / s, height: v.height / s)
    }

    /// What the session paints through: none where the whole slide shows.
    public var view: CGRect? {
        let v = visible
        guard v.width > 0, v.height > 0, v != slide else { return nil }
        return shown
    }

    /// Whether the slide and its margin are larger than the desk, so that a pan moves it.
    public var pans: Bool {
        let s = slide.size
        return s.width + 2 * Self.margin > desk.width + 0.5 || s.height + 2 * Self.margin > desk.height + 0.5
    }

    /// The same zoom on a desk `room` large: what the window lays the slide out by before the desk
    /// is told its size.
    public func placed(on room: CGSize) -> Zoom {
        var placed = self
        placed.place(on: room)
        return placed
    }

    /// As close as `z`, from the slide fitted to eight times as large, point `about` on the desk
    /// staying where it is: the pointer's, the fingers', or the middle of what shows.
    public mutating func zoom(to z: Double, about: CGPoint? = nil) {
        guard z > 0, canvas.width > 0, canvas.height > 0 else { return }
        let a = about ?? CGPoint(x: visible.midX, y: visible.midY)
        let before = max(scale, 1e-9)
        let at = CGPoint(x: (a.x - corner.x) / before, y: (a.y - corner.y) / before)
        level = min(Self.most, max(1, z))
        corner = CGPoint(x: a.x - at.x * scale, y: a.y - at.y * scale)
        settle()
    }

    /// A step closer (`1`) or farther (`-1`), about `about` on the desk.
    public mutating func step(_ by: Int, about: CGPoint? = nil) {
        let z = level
        let next =
            by > 0
            ? (Self.steps.first { $0 > z + 1e-6 } ?? Self.most)
            : (Self.steps.last { $0 < z - 1e-6 } ?? 1)
        zoom(to: next, about: about)
    }

    /// What shows moved `by` on the desk, points, the slide kept on it: whether it could move,
    /// which it cannot where the whole slide shows.
    @discardableResult
    public mutating func pan(by: CGVector) -> Bool {
        guard pans else { return false }
        corner = CGPoint(x: corner.x - by.dx, y: corner.y - by.dy)
        settle()
        return true
    }

    /// The slide fitted to the desk again, in its middle.
    public mutating func fit() {
        level = 1
        settle()
    }

    /// The canvas, another size (another format shown, or the deck's canvas changed): fitted to
    /// the desk. The same size changes nothing.
    public mutating func resize(to size: CGSize) {
        guard size != canvas else { return }
        canvas = size
        fit()
    }

    /// The desk, another size (the window resized, a pane shown or hidden): as close as it was,
    /// what showed in the middle in the middle still. The same size changes nothing.
    public mutating func place(on room: CGSize) {
        guard room != desk else { return }
        let (s, v) = (max(scale, 1e-9), visible)
        let middle =
            v.width > 0 && v.height > 0
            ? CGPoint(x: (v.midX - corner.x) / s, y: (v.midY - corner.y) / s)
            : CGPoint(x: canvas.width / 2, y: canvas.height / 2)
        desk = room
        corner = CGPoint(x: desk.width / 2 - middle.x * scale, y: desk.height / 2 - middle.y * scale)
        settle()
    }

    /// The slide kept on the desk: in its middle along a side where it fits with its margin, else
    /// panned no farther than the margin past its edge.
    private mutating func settle() {
        let size = slide.size
        corner = CGPoint(
            x: Self.kept(corner.x, size.width, on: desk.width), y: Self.kept(corner.y, size.height, on: desk.height))
    }

    private static func kept(_ at: CGFloat, _ length: CGFloat, on room: CGFloat) -> CGFloat {
        if length + 2 * margin <= room + 0.5 { return (room - length) / 2 }
        return min(margin, max(room - margin - length, at))
    }
}

/// The matches a find holds (PLAN 2.47), as the browser's find bar steps through them: each match
/// in each text found, in the order the deck shows them.
public struct Matches: Equatable, Sendable {
    /// Each match: the text it is in, as `find` lists them, and which match of that text.
    public struct Match: Equatable, Sendable {
        public let text: Int
        public let match: Int
    }

    public let all: [Match]

    public init(_ found: [Found]) {
        var all: [Match] = []
        for (t, f) in found.enumerated() {
            for m in f.matches.indices { all.append(Match(text: t, match: m)) }
        }
        self.all = all
    }

    /// The match after `now`, or the first, round to the start past the last; none where there
    /// are none.
    public func next(after now: Int?) -> Int? {
        guard !all.isEmpty else { return nil }
        guard let now else { return 0 }
        return (now + 1) % all.count
    }

    /// The match before `now`, or the last, round to the end before the first.
    public func previous(before now: Int?) -> Int? {
        guard !all.isEmpty else { return nil }
        guard let now else { return all.count - 1 }
        return (now - 1 + all.count) % all.count
    }
}

extension ScaenaSession {
    /// The formats the deck is laid out in besides its own canvas, as it writes them (`9:16`).
    public func formats() throws -> [String] { try call("formats") }

    /// Lay frames out in `format`, one of the deck's formats, or on its own canvas where none
    /// (SPEC §3.4): what the canvas, its boxes, its targets, and its grid are in from now on.
    public func setFormat(_ format: String?) throws {
        var asked: [String: JSONValue] = [:]
        if let format { asked["format"] = .string(format) }
        let _: JSONValue = try call("setFormat", .object(asked))
    }

    /// The patch that gives `node` a layout of its own in `format`, the format shown, where it
    /// stands now (ADR-0020, PLAN 2.85): a `place` with `anew`, from then on moved there alone.
    /// None where it has one there already.
    public func placingAnew(_ node: String, state: String, in format: String) throws -> [JSONValue]? {
        let inspected: JSONValue = try call("inspect", ["state": .string(state)])
        guard let props = inspected["nodes"]?[node], props != .null else {
            throw ScaenaError(message: "\(state) does not show \(node)")
        }
        if let own = props["formats"]?[format], own != .null { return nil }
        // Where it stands: its placement's own keys; one placed nowhere fills the grid's margins.
        var spot: [String: JSONValue] = [:]
        for key in ["col", "row", "in", "rect", "area", "index", "parent"] {
            if let value = props["at"]?[key], value != .null { spot[key] = value }
        }
        if spot.isEmpty { spot["in"] = .string("grid") }
        let op: [String: JSONValue] = [
            "op": "place", "node": .string(node), "at": .object(spot), "format": .string(format), "anew": true,
        ]
        return [.object(op)]
    }
}
