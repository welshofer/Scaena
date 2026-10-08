import CoreGraphics
import Foundation

/// How a box dropped on a node's targets snaps (ADR-0013): by as many tracks as it spans from the
/// nearest one (`move`), each edge to the nearest track (`resize`), into the slot it covers most
/// (`slot`), where it was left as a `rect` (`free`: on the theme's grid, an override, W301), or
/// among a stack's children (`order`).
public enum SnapMode: String, Codable, Sendable {
    case move
    case resize
    case slot
    case free
    case order
}

/// `[x, y, width, height]`, canvas units, as a rectangle.
private func rect(_ v: [Double]) throws -> CGRect {
    guard v.count == 4 else { throw ScaenaError(message: "a box is [x, y, width, height]") }
    return CGRect(x: v[0], y: v[1], width: v[2], height: v[3])
}

/// Where a node may go in a state at rest (ADR-0013, `inspect --targets`): what holds it, and so
/// how it is placed; the box its placement names now, which a drag moves; and the slots a
/// placement by name takes.
public struct Targets: Decodable, Sendable {
    /// `grid`, the theme's (by cells, a slot, or a `rect`); `stack` (by order); `cells`, a grid
    /// container's (by its cells or areas); or `frame` (by a `rect`).
    public let by: String
    /// The container that holds it.
    public let parent: String?
    /// The box its placement names now, canvas units.
    public let cell: CGRect
    /// The boxes a placement by name takes: the template's slots, then `canvas` and `grid`.
    public let slots: [String: CGRect]
    /// How a box dropped here snaps.
    public let snaps: [SnapMode]

    private enum Keys: String, CodingKey { case by, parent, cell, slots, snaps }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        by = try fields.decode(String.self, forKey: .by)
        parent = try fields.decodeIfPresent(String.self, forKey: .parent)
        cell = try rect(try fields.decode([Double].self, forKey: .cell))
        let named = try fields.decodeIfPresent([String: [Double]].self, forKey: .slots) ?? [:]
        slots = try named.mapValues(rect)
        snaps = try fields.decodeIfPresent([SnapMode].self, forKey: .snaps) ?? []
    }

    /// How a drag of the node, placed `at`, snaps, as the browser's canvas decides it: `resize` by
    /// a handle. Shift takes a node on the theme's grid off it, by a `rect`, or one off it back
    /// onto it. None: nothing places it that way.
    public func snap(at: JSONValue?, resize: Bool, shift: Bool) -> SnapMode? {
        let has = { (key: String) in at?[key].map { $0 != .null } ?? false }
        switch by {
        case "stack":
            return resize ? nil : .order
        case "frame":
            return .free
        case "cells":
            return has("area") ? (resize ? nil : .slot) : resize ? .resize : .move
        case "grid":
            if has("rect") != shift { return .free }
            if has("in") && !shift { return resize ? nil : .slot }
            return resize ? .resize : .move
        default:
            return nil
        }
    }
}

/// Where a box left on the canvas lands: the box a guide shows there, the patch that puts the node
/// there (`place` ops, each written where that placement lives; empty where it is there already),
/// and the guides it meets, each a line from one point to another.
public struct Snapped: Decodable, Sendable {
    public let cell: CGRect
    public let patch: [JSONValue]
    public let guides: [[CGPoint]]

    private enum Keys: String, CodingKey { case cell, patch, guides }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        cell = try rect(try fields.decode([Double].self, forKey: .cell))
        patch = try fields.decode([JSONValue].self, forKey: .patch)
        let lines = try fields.decodeIfPresent([[Double]].self, forKey: .guides) ?? []
        guides = lines.filter { $0.count == 4 }.map { [CGPoint(x: $0[0], y: $0[1]), CGPoint(x: $0[2], y: $0[3])] }
    }
}

extension ScaenaSession {
    /// Where `node` may go in `state` at rest.
    public func targets(state: String, node: String) throws -> Targets {
        try call("targets", ["state": .string(state), "node": .string(node)])
    }

    /// Where `node`'s box, left at `to` (canvas units), lands in `state` when it snaps `how`,
    /// and the patch that puts it there, or, to `fork` it, keeps it to `state`; off the grid,
    /// it goes first the least way that brings an edge, or its middle, onto another's within
    /// `reach` canvas units. None where nothing places it that way. It lays nothing out.
    public func snap(
        state: String, node: String, how: SnapMode, to: CGRect, fork: Bool = false, reach: Double = 0
    ) throws -> Snapped? {
        try call(
            "snap",
            [
                "state": .string(state), "node": .string(node), "how": .string(how.rawValue),
                "x": .number(to.minX), "y": .number(to.minY), "w": .number(to.width), "h": .number(to.height),
                "fork": .bool(fork), "reach": .number(reach),
            ])
    }

    /// Draw `nodes`, and what they hold, `by` canvas units from where they stand in frames at
    /// rest, laying nothing out: a drag as it moves. None: each where it stands.
    public func setMoving(_ nodes: [String], by: CGVector = .zero) throws {
        let _: JSONValue = try call(
            "setMoving",
            ["nodes": .array(nodes.map { .string($0) }), "dx": .number(by.dx), "dy": .number(by.dy)])
    }

    /// Draw frames at rest as `ops`, a patch, would make the deck, without making it: a resize
    /// that paused. None: the deck as it is.
    public func preview(_ ops: [JSONValue]?) throws {
        let patch: JSONValue = ops.map { JSONValue.array($0) } ?? .null
        let _: JSONValue = try call("preview", ["ops": patch])
    }

    /// The states `ops`, a patch, would change what shows in, by id: nothing made.
    public func reach(_ ops: [JSONValue]) throws -> [String] {
        try call("reach", ["ops": .array(ops)])
    }

    /// Each node `state` shows, and its placement there (`at`): what decides how a drag of it
    /// snaps, and whether it stands off the theme's grid.
    public func placements(state: String) throws -> [String: JSONValue] {
        let inspected: JSONValue = try call("inspect", ["state": .string(state)])
        var out: [String: JSONValue] = [:]
        for (node, props) in inspected["nodes"]?.object ?? [:] {
            out[node] = props["at"] ?? .null
        }
        return out
    }
}

extension JSONValue {
    /// Whether a placement stands off the theme's grid: a `rect` on the canvas, which no
    /// container holds. An override, which lint flags (W301).
    public var offGrid: Bool {
        guard let placed = self["rect"], placed != .null else { return false }
        return (self["parent"] ?? .null) == .null
    }
}
