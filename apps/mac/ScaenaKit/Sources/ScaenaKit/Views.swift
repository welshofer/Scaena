import CoreGraphics
import Foundation

// The canvas's views (PLAN 3.16), as the browser's preview keeps them: how close it is and what
// part of the canvas it shows (PLAN 2.46), the format it is laid out in (PLAN 2.62, 2.85), and the
// matches a find steps through (PLAN 2.47). Nothing here lays out or paints: the session paints
// through the part shown, and lays each format out once.

/// How close the canvas is shown (PLAN 2.46), as the browser's preview zooms: the part of it shown,
/// from the whole canvas to eight times as close, kept on the canvas.
public struct Zoom: Equatable, Sendable {
    /// The steps a step closer or farther goes by.
    public static let steps: [Double] = [1, 1.5, 2, 3, 4, 6, 8]
    /// The closest it goes.
    public static let most = 8.0

    /// The canvas, canvas units.
    public private(set) var canvas: CGSize
    /// The part of the canvas shown, canvas units.
    public private(set) var shown: CGRect

    public init(canvas: CGSize) {
        self.canvas = canvas
        shown = CGRect(origin: .zero, size: canvas)
    }

    /// How close: 1 shows the whole canvas.
    public var level: Double { shown.width > 0 ? Double(canvas.width / shown.width) : 1 }

    /// What the session paints through: none where the whole canvas shows.
    public var view: CGRect? { level > 1 + 1e-9 ? shown : nil }

    /// `next` shown, as close as it asks, from the whole canvas to eight times as close, kept on
    /// the canvas.
    public mutating func look(_ next: CGRect) {
        guard canvas.width > 0, canvas.height > 0 else { return }
        let z = min(Self.most, max(1, Double(canvas.width) / max(Double(next.width), 1e-9)))
        let (w, h) = (Double(canvas.width) / z, Double(canvas.height) / z)
        let x = min(max(Double(next.minX), 0), Double(canvas.width) - w)
        let y = min(max(Double(next.minY), 0), Double(canvas.height) - h)
        shown = CGRect(x: x, y: y, width: w, height: h)
    }

    /// As close as `z`, canvas point `about` staying where it is on the screen: the pointer's, or
    /// the middle of what is shown.
    public mutating func zoom(to z: Double, about: CGPoint? = nil) {
        guard z > 0, shown.width > 0, shown.height > 0 else { return }
        let a = about ?? CGPoint(x: shown.midX, y: shown.midY)
        let fx = Double((a.x - shown.minX) / shown.width)
        let fy = Double((a.y - shown.minY) / shown.height)
        let (w, h) = (Double(canvas.width) / z, Double(canvas.height) / z)
        look(CGRect(x: Double(a.x) - fx * w, y: Double(a.y) - fy * h, width: w, height: h))
    }

    /// A step closer (`1`) or farther (`-1`), about `about`.
    public mutating func step(_ by: Int, about: CGPoint? = nil) {
        let z = level
        let next =
            by > 0
            ? (Self.steps.first { $0 > z + 1e-6 } ?? Self.most)
            : (Self.steps.last { $0 < z - 1e-6 } ?? 1)
        zoom(to: next, about: about)
    }

    /// What is shown moved `by`, canvas units, kept on the canvas.
    public mutating func pan(by: CGVector) {
        look(shown.offsetBy(dx: by.dx, dy: by.dy))
    }

    /// The whole canvas again.
    public mutating func fit() {
        shown = CGRect(origin: .zero, size: canvas)
    }

    /// The canvas, another size (another format shown, or the deck's canvas changed): the whole of
    /// it shown. The same size changes nothing.
    public mutating func resize(to size: CGSize) {
        guard size != canvas else { return }
        canvas = size
        fit()
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
