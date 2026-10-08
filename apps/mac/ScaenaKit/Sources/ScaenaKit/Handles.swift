import CScaena
import CoreGraphics
import Foundation

// Handles and views (PLAN 3.16), as the browser's canvas reads them: a shape's points and a rect's
// corner (PLAN 2.68), an image's crop and focal point (PLAN 2.45, 2.74), the turn of a node about
// its anchor (PLAN 2.51), the theme's grid (PLAN 2.57), the part of the canvas frames are painted
// through (PLAN 2.46), a format beside the canvas (PLAN 2.62), and the deck's texts found and
// replaced (PLAN 2.47). Nothing here lays out: each is read from the engine, and each handle let go
// is one `choose`, written where the value lives.

/// A shape's outline in a state at rest (PLAN 2.68): its points, a rect's corner, and the theme's
/// radius steps.
public struct Outline: Decodable, Sendable {
    public let node: String
    /// `rect`, `ellipse`, `line`, `arrow`, `polygon`, or `path`.
    public let kind: String
    /// Its box as laid out, canvas units.
    public let rect: CGRect
    /// The map from its box as laid out to where it is drawn, where something turns it.
    public let transform: [Double]?
    /// A line's, an arrow's, or a polygon's points, fractions of its box.
    public let points: [CGPoint]
    /// The fewest points its kind keeps.
    public let fewest: Int
    /// A rect's corner radius, canvas units; and the theme's radius steps, in order.
    public let radius: Double?
    public let radii: [Double]

    private enum Keys: String, CodingKey { case node, kind, rect, transform, points, fewest, radius, radii }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        node = try fields.decode(String.self, forKey: .node)
        kind = try fields.decode(String.self, forKey: .kind)
        rect = try ScaenaKit.rect(try fields.decode([Double].self, forKey: .rect))
        transform = try fields.decodeIfPresent([Double].self, forKey: .transform)
        let points = try fields.decodeIfPresent([[Double]].self, forKey: .points) ?? []
        self.points = points.filter { $0.count == 2 }.map { CGPoint(x: $0[0], y: $0[1]) }
        fewest = try fields.decodeIfPresent(Int.self, forKey: .fewest) ?? 0
        radius = try fields.decodeIfPresent(Double.self, forKey: .radius)
        radii = try fields.decodeIfPresent([Double].self, forKey: .radii) ?? []
    }

    /// Where each point is drawn, canvas units.
    public var drawn: [CGPoint] { points.map { onCanvas($0) } }

    /// Where `fraction`, a point of its box as a fraction of it, is drawn, canvas units.
    public func onCanvas(_ fraction: CGPoint) -> CGPoint { through(transform, inBox(fraction)) }

    /// The edges its points make: a polygon's closes.
    public var edges: Int { kind == "polygon" ? points.count : max(0, points.count - 1) }

    /// The middle of each edge, drawn: where a press adds a point.
    public var middles: [CGPoint] {
        (0..<edges).map { i in
            let (a, b) = (points[i], points[(i + 1) % points.count])
            return onCanvas(CGPoint(x: (a.x + b.x) / 2, y: (a.y + b.y) / 2))
        }
    }

    /// The point of its box drawn at `point`, canvas units, as a fraction of the box to a
    /// hundredth, kept to it: where a point dragged there goes.
    public func fraction(at point: CGPoint) -> CGPoint {
        let p = back(transform, point)
        let x = rect.width > 0 ? Double((p.x - rect.minX) / rect.width) : 0
        let y = rect.height > 0 ? Double((p.y - rect.minY) / rect.height) : 0
        return CGPoint(x: hundredth(x), y: hundredth(y))
    }

    /// Its points with point `index` moved to `to`, a fraction of the box.
    public func moving(_ index: Int, to: CGPoint) -> [CGPoint] {
        var moved = points
        if moved.indices.contains(index) { moved[index] = to }
        return moved
    }

    /// Its points with one added at the middle of the edge after point `index`, to a hundredth.
    public func adding(after index: Int) -> [CGPoint] {
        guard points.indices.contains(index) else { return points }
        let (a, b) = (points[index], points[(index + 1) % points.count])
        var added = points
        added.insert(CGPoint(x: hundredth(Double(a.x + b.x) / 2), y: hundredth(Double(a.y + b.y) / 2)), at: index + 1)
        return added
    }

    /// Its points with point `index` taken away; none where its kind keeps no fewer: a line or an
    /// arrow two, a polygon three.
    public func removing(_ index: Int) -> [CGPoint]? {
        guard points.indices.contains(index), points.count > fewest else { return nil }
        var kept = points
        kept.remove(at: index)
        return kept
    }

    /// Where a rect's corner handle stands, drawn: in from its top left corner as far as its
    /// radius, clear of the corner's resize handle (`clear`, canvas units), inside its box.
    public func corner(clear: Double) -> CGPoint {
        let d = min(max(radius ?? 0, clear), Double(min(rect.width, rect.height)) / 2)
        return through(transform, CGPoint(x: Double(rect.minX) + d, y: Double(rect.minY) + d))
    }

    /// The radius step nearest `radius`, canvas units: the first of those as near.
    public func step(nearest radius: Double) -> Int {
        var best = 0
        for (i, v) in radii.enumerated() where abs(v - radius) < abs(radii[best] - radius) - 1e-3 {
            best = i
        }
        return best
    }

    /// The step a corner handle dragged from `from` to `to`, canvas units, rounds the corners to:
    /// its radius, as far again as the pointer went along the box's diagonal, kept to half its
    /// shorter side.
    public func step(from: CGPoint, to: CGPoint) -> Int {
        let (a, b) = (back(transform, from), back(transform, to))
        let half = Double(min(rect.width, rect.height)) / 2
        let went = Double(b.x - a.x + b.y - a.y) / 2
        return step(nearest: min(half, max(0, (radius ?? 0) + went)))
    }

    /// Whether step `step` rounds its corners otherwise than they are.
    public func rounds(to step: Int) -> Bool {
        radii.indices.contains(step) && abs((radius ?? 0) - radii[step]) >= 0.01
    }

    private func inBox(_ f: CGPoint) -> CGPoint {
        CGPoint(x: rect.minX + f.x * rect.width, y: rect.minY + f.y * rect.height)
    }
}

/// An image's framing in a state at rest (PLAN 2.74): its box, where its whole image and the part
/// that shows are drawn, its crop, fractions of the image, and its focal point, fractions of the
/// crop.
public struct Framing: Decodable, Sendable {
    public let node: String
    public let rect: CGRect
    public let transform: [Double]?
    /// Where the whole image is drawn, and the part of it that shows, canvas units as laid out.
    public let whole: CGRect
    public let shown: CGRect
    /// `[x, y, width, height]`, fractions of the image.
    public let crop: [Double]
    public let focal: CGPoint
    /// `cover`, `contain`, or `fill`.
    public let fit: String
    /// The image, in pixels.
    public let size: CGSize

    private enum Keys: String, CodingKey { case node, rect, transform, whole, shown, crop, focal, fit, size }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        node = try fields.decode(String.self, forKey: .node)
        rect = try ScaenaKit.rect(try fields.decode([Double].self, forKey: .rect))
        transform = try fields.decodeIfPresent([Double].self, forKey: .transform)
        whole = try ScaenaKit.rect(try fields.decode([Double].self, forKey: .whole))
        shown = try ScaenaKit.rect(try fields.decode([Double].self, forKey: .shown))
        crop = try fields.decode([Double].self, forKey: .crop)
        let focal = try fields.decode([Double].self, forKey: .focal)
        self.focal = focal.count == 2 ? CGPoint(x: focal[0], y: focal[1]) : CGPoint(x: 0.5, y: 0.5)
        fit = try fields.decodeIfPresent(String.self, forKey: .fit) ?? "cover"
        let size = try fields.decodeIfPresent([Double].self, forKey: .size) ?? []
        self.size = size.count == 2 ? CGSize(width: size[0], height: size[1]) : .zero
    }

    /// A side of the part that shows.
    public enum Side: CaseIterable, Sendable {
        case top, right, bottom, left
    }

    /// The least a crop keeps of the image, each way.
    public static let least = 0.05

    /// Where the crop handle of `side` stands, drawn: inside the middle of that side of the part
    /// that shows, `inset` canvas units in, or less on a small image.
    public func handle(_ side: Side, inset: Double) -> CGPoint {
        let s = shown
        let i = CGFloat(min(inset, Double(s.width) / 4, Double(s.height) / 4))
        let p: CGPoint =
            switch side {
            case .top: CGPoint(x: s.midX, y: s.minY + i)
            case .right: CGPoint(x: s.maxX - i, y: s.midY)
            case .bottom: CGPoint(x: s.midX, y: s.maxY - i)
            case .left: CGPoint(x: s.minX + i, y: s.midY)
            }
        return through(transform, p)
    }

    /// Where the focal point's handle stands, drawn: the point of the box it lines up with, as
    /// CSS `object-position` places it.
    public var focalHandle: CGPoint { focalPoint(self.focal) }

    /// Where focal point `focal` stands, drawn.
    public func focalPoint(_ focal: CGPoint) -> CGPoint {
        through(transform, CGPoint(x: rect.minX + focal.x * rect.width, y: rect.minY + focal.y * rect.height))
    }

    /// The part of the whole image `crop` keeps, canvas units as laid out.
    public func kept(_ crop: [Double]) -> CGRect {
        guard crop.count == 4 else { return shown }
        let w = whole
        return CGRect(
            x: Double(w.minX) + crop[0] * Double(w.width), y: Double(w.minY) + crop[1] * Double(w.height),
            width: crop[2] * Double(w.width), height: crop[3] * Double(w.height))
    }

    /// The crop `side` dragged from `from` to `to`, canvas units, asks for: that side as far as
    /// the pointer went, kept to the image and to a twentieth of it at least, each a fraction of the
    /// image to a thousandth.
    public func cropped(_ side: Side, from: CGPoint, to: CGPoint) -> [Double] {
        guard crop.count == 4, whole.width > 0, whole.height > 0 else { return crop }
        let (a, b) = (back(transform, from), back(transform, to))
        let dx = Double((b.x - a.x) / whole.width)
        let dy = Double((b.y - a.y) / whole.height)
        return cropped(side, by: CGVector(dx: dx, dy: dy))
    }

    /// The crop `side` moved `by`, fractions of the image, asks for: what an arrow key on its
    /// handle makes, a hundredth or a tenth at a time.
    public func cropped(_ side: Side, by: CGVector) -> [Double] {
        guard crop.count == 4 else { return crop }
        let (x, y, w, h) = (crop[0], crop[1], crop[2], crop[3])
        let (dx, dy) = (Double(by.dx), Double(by.dy))
        let least = Self.least
        let out: [Double]
        switch side {
        case .left:
            let nx = min(x + w - least, max(0, x + dx))
            out = [nx, y, x + w - nx, h]
        case .right: out = [x, y, min(1 - x, max(least, w + dx)), h]
        case .top:
            let ny = min(y + h - least, max(0, y + dy))
            out = [x, ny, w, y + h - ny]
        case .bottom: out = [x, y, w, min(1 - y, max(least, h + dy))]
        }
        return out.map { thousandth(min(1, max(0, $0))) }
    }

    /// Whether `crop` crops the image otherwise than it is.
    public func changes(crop: [Double]) -> Bool { !same(crop, self.crop) }

    /// Whether `crop` keeps the whole image: a crop of none.
    public static func keepsWhole(_ crop: [Double]) -> Bool { same(crop, [0, 0, 1, 1]) }

    /// The focal point a handle dragged to `point`, canvas units, asks for: the point of the box
    /// there, fractions of the box to a thousandth, kept to it.
    public func focal(at point: CGPoint) -> CGPoint {
        let p = back(transform, point)
        let x = rect.width > 0 ? Double((p.x - rect.minX) / rect.width) : Double(self.focal.x)
        let y = rect.height > 0 ? Double((p.y - rect.minY) / rect.height) : Double(self.focal.y)
        return CGPoint(x: thousandth(min(1, max(0, x))), y: thousandth(min(1, max(0, y))))
    }

    /// Whether `focal` moves the focal point from where it is.
    public func changes(focal: CGPoint) -> Bool {
        !same([Double(focal.x), Double(focal.y)], [Double(self.focal.x), Double(self.focal.y)])
    }
}

/// Two crops or points alike, within what a thousandth rounds away.
private func same(_ a: [Double], _ b: [Double]) -> Bool {
    a.count == b.count && zip(a, b).allSatisfy { abs($0 - $1) < 5e-4 }
}

/// The theme's grid in the format shown (PLAN 2.57), canvas units: the canvas, its column and row
/// tracks, each from its start to its end, and the baseline grid's lines.
public struct GridLines: Decodable, Sendable {
    public let canvas: CGSize
    public let columns: [ClosedRange<Double>]
    public let rows: [ClosedRange<Double>]
    public let baselines: [Double]

    private enum Keys: String, CodingKey { case canvas, columns, rows, baselines }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let canvas = try fields.decode([Double].self, forKey: .canvas)
        self.canvas = canvas.count == 2 ? CGSize(width: canvas[0], height: canvas[1]) : .zero
        let track = { (pair: [Double]) -> ClosedRange<Double>? in
            pair.count == 2 && pair[0] <= pair[1] ? pair[0]...pair[1] : nil
        }
        columns = try fields.decode([[Double]].self, forKey: .columns).compactMap(track)
        rows = try fields.decode([[Double]].self, forKey: .rows).compactMap(track)
        baselines = try fields.decodeIfPresent([Double].self, forKey: .baselines) ?? []
    }
}

/// A text the deck writes that a find matches (PLAN 2.47): where it lives, the state it is found
/// in and those that show it from there, its words, and each match in them.
public struct Found: Decodable, Sendable {
    /// `text`, or what else writes words: a beat's `claim`, a state's `notes`, a node's `alt`.
    public let kind: String
    public let node: String?
    public let beat: String?
    public let state: String?
    public let states: [String]
    /// A JSON pointer to where it is written.
    public let lives: String
    public let text: String
    /// Each match, `[from, to]` in characters (Unicode scalar values).
    public let matches: [[Int]]

    private enum Keys: String, CodingKey { case kind, node, beat, state, states, lives, text, matches }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        kind = try fields.decodeIfPresent(String.self, forKey: .kind) ?? "text"
        node = try fields.decodeIfPresent(String.self, forKey: .node)
        beat = try fields.decodeIfPresent(String.self, forKey: .beat)
        state = try fields.decodeIfPresent(String.self, forKey: .state)
        states = try fields.decodeIfPresent([String].self, forKey: .states) ?? []
        lives = try fields.decode(String.self, forKey: .lives)
        text = try fields.decode(String.self, forKey: .text)
        matches = try fields.decodeIfPresent([[Int]].self, forKey: .matches) ?? []
    }
}

/// The patches the handles make (PLAN 2.51, 2.68, 2.74), as the browser's canvas makes them: each
/// one `choose`, written where the value lives, or kept to the state shown with `fork`.
public enum Handling {
    static func choose(_ prop: String, _ value: JSONValue, node: String, state: String, fork: Bool) -> [JSONValue] {
        var op: [String: JSONValue] = [
            "op": "choose", "node": .string(node), "prop": .string(prop), "value": value, "state": .string(state),
        ]
        if fork { op["fork"] = true }
        return [.object(op)]
    }

    /// `node` turned to `degrees` about its anchor.
    public static func turning(_ node: String, to degrees: Double, in state: String, fork: Bool = false) -> [JSONValue] {
        choose("transform/rotate", .number(degrees), node: node, state: state, fork: fork)
    }

    /// A shape's points, fractions of its box.
    public static func reshaping(_ node: String, to points: [CGPoint], in state: String, fork: Bool = false) -> [JSONValue] {
        let value = JSONValue.array(points.map { .array([.number(Double($0.x)), .number(Double($0.y))]) })
        return choose("points", value, node: node, state: state, fork: fork)
    }

    /// A rect's corners rounded to the theme's radius step `step`.
    public static func rounding(_ node: String, to step: Int, in state: String, fork: Bool = false) -> [JSONValue] {
        choose("radius", .string("radius.\(step)"), node: node, state: state, fork: fork)
    }

    /// An image cropped to `crop`, fractions of the image; none, or one that keeps the whole
    /// image, is no crop.
    public static func cropping(_ node: String, to crop: [Double]?, in state: String, fork: Bool = false) -> [JSONValue] {
        var value = JSONValue.null
        if let crop, !Framing.keepsWhole(crop) { value = .array(crop.map { .number($0) }) }
        return choose("crop", value, node: node, state: state, fork: fork)
    }

    /// An image's focal point, fractions of its crop.
    public static func focusing(_ node: String, on focal: CGPoint, in state: String, fork: Bool = false) -> [JSONValue] {
        choose("focal", .array([.number(Double(focal.x)), .number(Double(focal.y))]), node: node, state: state, fork: fork)
    }
}

/// A node's turn in a state, resolved (PLAN 2.51): what its rotate handle turns.
public struct Turned: Equatable, Sendable {
    /// Degrees, clockwise on the canvas.
    public let rotate: Double
    /// The point it turns about, fractions of its box.
    public let anchor: CGPoint

    public init(rotate: Double = 0, anchor: CGPoint = CGPoint(x: 0.5, y: 0.5)) {
        self.rotate = rotate
        self.anchor = anchor
    }
}

/// A turn by the rotate handle under way (PLAN 2.51), as the browser's canvas keeps one: about
/// `pivot`, where the node's anchor is drawn, from `start` degrees to `now`, as the pointer goes
/// round from where it pressed. Past a half turn it keeps going.
public struct Turn: Sendable {
    public let node: String
    public let pivot: CGPoint
    public let start: Double
    /// The angle it turns to now: whole degrees, or with Shift fifteens.
    public private(set) var now: Double
    /// The pointer's last angle about the pivot, radians, and how far it has gone round since the
    /// press, degrees.
    private var last: Double
    private var round = 0.0
    /// -1 where what holds the node mirrors it, so that its own clockwise turn goes the other way
    /// on the canvas.
    private let way: Double

    /// A turn of the node in `box`, turned as `turned` says, pressed at `from`, canvas units;
    /// `holder`, the map of what holds it, which may mirror it.
    public init(_ box: NodeBox, turned: Turned, from: CGPoint, holder: [Double]? = nil) {
        node = box.node
        let r = box.rect.count == 4 ? box.rect : [0, 0, 0, 0]
        let anchor = CGPoint(x: r[0] + Double(turned.anchor.x) * r[2], y: r[1] + Double(turned.anchor.y) * r[3])
        pivot = through(box.transform, anchor)
        start = turned.rotate
        now = turned.rotate
        last = atan2(Double(from.y - pivot.y), Double(from.x - pivot.x))
        let m = holder ?? []
        way = m.count == 6 && m[0] * m[3] - m[1] * m[2] < 0 ? -1 : 1
    }

    /// The pointer at `at`, canvas units: the angle follows it round, in whole degrees, or with
    /// `snap` in fifteens.
    public mutating func move(to at: CGPoint, snap: Bool) {
        let angle = atan2(Double(at.y - pivot.y), Double(at.x - pivot.x))
        var step = angle - last
        if step > .pi { step -= 2 * .pi }
        if step < -.pi { step += 2 * .pi }
        last = angle
        round += step * 180 / .pi
        let to = start + round * way
        // Halves round up, as the browser's `Math.round` rounds them.
        now = snap ? ((to / 15) + 0.5).rounded(.down) * 15 : (to + 0.5).rounded(.down)
    }

    /// How far it has turned the node on the canvas, degrees: what its outline is turned by
    /// about the pivot as it goes.
    public var turning: Double { (now - start) * way }
}

extension NodeBox {
    /// A point of its box as laid out, where it is drawn, canvas units.
    public func onCanvas(_ point: CGPoint) -> CGPoint { through(transform, point) }

    /// A point on the canvas, read back through what draws its box there: where it is in the box
    /// as laid out.
    public func laidOut(_ point: CGPoint) -> CGPoint { back(transform, point) }

    /// Where the rotate handle stands, canvas units: `arm` beyond the middle of its top edge as
    /// drawn, away from its middle; and that middle, where the handle's arm begins.
    public func turnHandle(arm: Double) -> (at: CGPoint, top: CGPoint)? {
        let c = corners
        guard c.count == 4 else { return nil }
        let top = CGPoint(x: (c[0].x + c[1].x) / 2, y: (c[0].y + c[1].y) / 2)
        let middle = CGPoint(x: (c[0].x + c[2].x) / 2, y: (c[0].y + c[2].y) / 2)
        let (dx, dy) = (Double(top.x - middle.x), Double(top.y - middle.y))
        let length = hypot(dx, dy)
        let (ux, uy) = length > 1e-6 ? (dx / length, dy / length) : (0, -1)
        return (CGPoint(x: Double(top.x) + ux * arm, y: Double(top.y) + uy * arm), top)
    }
}

/// `by`, a move on the canvas, as a move of what `map` draws: read back through the map's turn and
/// scale, its shift aside (PLAN 2.51). A drag of a node moves it so far as what holds it lays it
/// out, and a handle resizes it along its own sides.
public func across(_ map: [Double]?, _ by: CGVector) -> CGVector {
    let a = back(map, CGPoint(x: by.dx, y: by.dy))
    let o = back(map, .zero)
    return CGVector(dx: a.x - o.x, dy: a.y - o.y)
}

/// `v` to a hundredth, kept to 0…1: a point of a box, as a person would write it.
func hundredth(_ v: Double) -> Double { (min(1, max(0, v)) * 100).rounded() / 100 }

/// `v` to a thousandth, as a person would write it.
func thousandth(_ v: Double) -> Double { (v * 1000).rounded() / 1000 }

extension ScaenaSession {
    /// Shape `node`'s outline in `state` at rest; none for a node the state does not draw, or one
    /// that is no shape.
    public func outline(state: String, node: String) throws -> Outline? {
        try call("outline", ["state": .string(state), "node": .string(node)])
    }

    /// Image `node`'s framing in `state` at rest; none for a node the state does not draw, or one
    /// that is no image.
    public func framing(state: String, node: String) throws -> Framing? {
        try call("framing", ["state": .string(state), "node": .string(node)])
    }

    /// The point of image `node` drawn under `point` in `state` at rest, fractions of the part its
    /// crop keeps: what a focal point picked there is (PLAN 2.45). None off the image.
    public func focal(state: String, node: String, at point: CGPoint) throws -> CGPoint? {
        let asked: [String: JSONValue] = [
            "state": .string(state), "node": .string(node), "x": .number(Double(point.x)), "y": .number(Double(point.y)),
        ]
        let at: [Double]? = try call("focalAt", .object(asked))
        guard let at, at.count == 2 else { return nil }
        return CGPoint(x: at[0], y: at[1])
    }

    /// Node `node`'s turn in `state`, resolved: its `rotate` and its `anchor`, the middle of its
    /// box where it names none (PLAN 2.51).
    public func turned(state: String, node: String) throws -> Turned {
        let inspected: JSONValue = try call("inspect", ["state": .string(state)])
        let transform = inspected["nodes"]?[node]?["transform"]
        let anchor = transform?["anchor"]?.array?.compactMap(\.number) ?? []
        return Turned(
            rotate: transform?["rotate"]?.number ?? 0,
            anchor: anchor.count == 2 ? CGPoint(x: anchor[0], y: anchor[1]) : CGPoint(x: 0.5, y: 0.5))
    }

    /// The theme's grid in the format shown.
    public func grid() throws -> GridLines { try call("grid") }

    /// Paint the canvas's frames through `view`, canvas units: the part a zoomed canvas shows
    /// (PLAN 2.46); none, the whole canvas.
    public func setView(_ view: CGRect?) throws {
        var asked: [String: JSONValue] = [:]
        if let view {
            asked["view"] = .array([view.minX, view.minY, view.width, view.height].map { .number(Double($0)) })
        }
        let _: JSONValue = try call("setView", .object(asked))
    }

    /// Each text the deck writes that `text` matches (PLAN 2.47): in any case unless `matchCase`,
    /// and only as whole words with `words`.
    public func find(_ text: String, matchCase: Bool = false, words: Bool = false) throws -> [Found] {
        try call("find", ["query": query(text, matchCase: matchCase, words: words)])
    }

    /// The patch that replaces what `text` matches with `with`: every match, or with `one`, the
    /// match `one.match` of the text `one.text` as `find` lists them.
    public func replacing(
        _ text: String, with: String, matchCase: Bool = false, words: Bool = false, one: (text: Int, match: Int)? = nil
    ) throws -> [JSONValue] {
        var args: [String: JSONValue] = ["query": query(text, matchCase: matchCase, words: words), "with": .string(with)]
        if let one { args["one"] = .array([.number(Double(one.text)), .number(Double(one.match))]) }
        return try call("replacing", .object(args))
    }

    private func query(_ text: String, matchCase: Bool, words: Bool) -> JSONValue {
        ["find": .string(text), "case": .bool(matchCase), "words": .bool(words)]
    }

    /// `state` at `ms` (infinity: at rest) in `format`, one of the deck's formats, or its own
    /// canvas where none, painted by the CPU painter `height` pixels high (PLAN 2.62): what the
    /// formats beside the canvas show.
    public func pixels(in format: String?, state: String, at ms: Double = .infinity, height: Int) throws -> Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let high = UInt32(max(height, 1))
        // A format by its name, or none: the C string made only where there is one.
        let painted: ScaenaPixels
        if let format {
            painted = scaena_pixels_in(handle, format, state, ms, high, &error)
        } else {
            painted = scaena_pixels_in(handle, nil, state, ms, high, &error)
        }
        let rgba = try Self.take(painted.bytes, error)
        return Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height))
    }
}
