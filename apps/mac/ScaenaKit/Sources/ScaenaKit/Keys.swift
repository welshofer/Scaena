import CoreGraphics
import Foundation

// The canvas by keys alone and for VoiceOver (PLAN 3.17), as the browser's canvas answers keys
// (PLAN 2.75, 2.89) and its reader hears a state (PLAN 2.8, 2.56): the order Tab selects in, where
// an arrow key steps a node, the handles Enter goes onto, and what each node says to a screen
// reader. Each step is the patch its drag makes; nothing here lays out.

/// What a reader hears of a node a state shows (SPEC §3.12), in turn: its role, and its words or
/// what it shows, as the reading HTML says it.
public struct ReadPart: Decodable, Equatable, Sendable {
    public let node: String
    /// `heading`, `paragraph`, `figure`, or `table`.
    public let role: String
    /// A heading's level, 1 the highest.
    public let level: Int?
    /// Its words, or its alt text instead; a figure's alt text; a table's cells row by row.
    public let text: String
    /// BCP 47, where it is not the deck's language.
    public let lang: String?
}

extension ScaenaSession {
    /// How `state` at rest reads, a part for each node read, in turn, in the format shown (PLAN
    /// 3.17): what VoiceOver speaks of the canvas.
    public func reads(state: String) throws -> [ReadPart] { try call("reads", ["state": .string(state)]) }
}

/// The nodes of a state in reading order (PLAN 2.75), as the browser's Tab steps through them:
/// those container `parent` holds (none, the canvas), as they are painted, a container where the
/// first it holds is, and none that is locked, which the keys pass over as a pointer does.
public func readingOrder(_ boxes: [NodeBox], in parent: String?) -> [String] {
    var painted: [String: Int] = [:]
    for (i, b) in boxes.enumerated() where b.draws && painted[b.node] == nil { painted[b.node] = i }
    func first(_ node: String, _ depth: Int) -> Int {
        if let own = painted[node] { return own }
        guard depth < 64 else { return Int.max }
        return boxes.filter { $0.parent == node }.map { first($0.node, depth + 1) }.min() ?? Int.max
    }
    var seen: Set<String> = []
    let here = boxes.filter { $0.parent == parent && $0.locked == nil }.map(\.node).filter { seen.insert($0).inserted }
    return here.sorted { a, b in
        let (fa, fb) = (first(a, 0), first(b, 0))
        return fa != fb ? fa < fb : a < b
    }
}

extension Targets {
    /// Where an arrow key takes a node, `dx` and `dy` each -1, 0, or 1 (PLAN 2.75), snapped `how`,
    /// as the browser's keys step it: a move to the next track's start past the box's, a resize
    /// (`grow`) its end to the next track's end; off the grid, a unit; in a stack, past the middle of
    /// the child before or after it, whose box `box` gives. None where it goes nowhere: at the edge.
    public func stepped(
        _ node: String, dx: Int, dy: Int, how: SnapMode, grow: Bool, box: (String) -> CGRect?
    ) -> CGRect? {
        let (x, y, w, h) = (Double(cell.minX), Double(cell.minY), Double(cell.width), Double(cell.height))
        let (sx, sy) = (Double(dx), Double(dy))
        switch how {
        case .order:
            guard let at = flow.firstIndex(of: node) else { return nil }
            let n = at + (dx != 0 ? dx : dy)
            guard flow.indices.contains(n), let next = box(flow[n]) else { return nil }
            let s = sx + sy
            return CGRect(
                x: Double(next.midX) + s - w / 2, y: Double(next.midY) + s - h / 2, width: w, height: h)
        case .free:
            return grow ? CGRect(x: x, y: y, width: w + sx, height: h + sy) : CGRect(x: x + sx, y: y + sy, width: w, height: h)
        case .move, .resize:
            // The next track's start (a move) or end (a resize) past the box's start or end.
            func step(_ tracks: [ClosedRange<Double>], from: Double, _ s: Double, end: Bool) -> Double? {
                let lines = tracks.map { end ? $0.upperBound : $0.lowerBound }
                let found = s > 0 ? lines.first { $0 > from + 0.5 } : lines.last { $0 < from - 0.5 }
                return found.map { $0 - from }
            }
            let ddx = dx == 0 ? 0 : step(columns, from: grow ? x + w : x, sx, end: grow)
            let ddy = dy == 0 ? 0 : step(rows, from: grow ? y + h : y, sy, end: grow)
            guard let ddx, let ddy else { return nil }
            return grow
                ? CGRect(x: x, y: y, width: w + ddx, height: h + ddy) : CGRect(x: x + ddx, y: y + ddy, width: w, height: h)
        case .slot:
            return nil
        }
    }
}

/// A handle of the node selected that the keys work (PLAN 2.75): a shape's point or a rect's
/// corner, an image's crop side or its focal point, in the order Tab steps through them.
public enum KeyedHandle: Equatable, Sendable {
    case point(Int)
    case corner
    case crop(Framing.Side)
    case focal

    /// What the status says it is.
    public var label: String {
        switch self {
        case .point(let i): "point \(i + 1)"
        case .corner: "its corner"
        case .crop(let side): "the crop's \(side)"
        case .focal: "its focal point"
        }
    }

    /// The handles of a node with `outline` or `framing`, in order: a shape's points, a rect's
    /// corner; an image's crop sides, then its focal point.
    public static func of(outline: Outline?, framing: Framing?) -> [KeyedHandle] {
        var out: [KeyedHandle] = []
        if let o = outline, ["line", "arrow", "polygon"].contains(o.kind) {
            out += o.points.indices.map { .point($0) }
        }
        if let o = outline, o.kind == "rect", !o.radii.isEmpty { out.append(.corner) }
        if framing != nil { out += Framing.Side.allCases.map { .crop($0) } + [.focal] }
        return out
    }

    /// The patch an arrow key on it makes, `dx` and `dy` each -1, 0, or 1, a hundredth of the box
    /// (with `far`, a tenth), a corner a radius step, as its drag would make it (PLAN 2.75); none
    /// where it changes nothing. A crop side moves only across its side.
    public func nudged(
        dx: Int, dy: Int, far: Bool, outline: Outline?, framing: Framing?, state: String, fork: Bool
    ) -> [JSONValue]? {
        let by = far ? 0.1 : 0.01
        switch self {
        case .point(let i):
            guard let o = outline, o.points.indices.contains(i) else { return nil }
            let p = o.points[i]
            let moved = CGPoint(x: hundredth(Double(p.x) + Double(dx) * by), y: hundredth(Double(p.y) + Double(dy) * by))
            guard moved != p else { return nil }
            return Handling.reshaping(o.node, to: o.moving(i, to: moved), in: state, fork: fork)
        case .corner:
            guard let o = outline, !o.radii.isEmpty else { return nil }
            let now = o.step(nearest: o.radius ?? 0)
            let step = min(o.radii.count - 1, max(0, now + (dx > 0 || dy < 0 ? 1 : -1)))
            guard o.rounds(to: step) else { return nil }
            return Handling.rounding(o.node, to: step, in: state, fork: fork)
        case .crop(let side):
            guard let f = framing else { return nil }
            let across = side == .left || side == .right
            guard across ? dx != 0 : dy != 0 else { return nil }
            let crop = f.cropped(side, by: CGVector(dx: Double(dx) * by, dy: Double(dy) * by))
            guard f.changes(crop: crop) else { return nil }
            return Handling.cropping(f.node, to: crop, in: state, fork: fork)
        case .focal:
            guard let f = framing else { return nil }
            let at = CGPoint(
                x: thousandth(min(1, max(0, Double(f.focal.x) + Double(dx) * by))),
                y: thousandth(min(1, max(0, Double(f.focal.y) + Double(dy) * by))))
            guard f.changes(focal: at) else { return nil }
            return Handling.focusing(f.node, on: at, in: state, fork: fork)
        }
    }
}
