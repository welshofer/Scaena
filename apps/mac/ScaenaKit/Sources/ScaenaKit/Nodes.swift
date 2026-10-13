import CoreGraphics
import Foundation

/// What a deck may have inserted (PLAN 2.34): what a menu says, the node it adds, and what its
/// id starts from.
public struct Insert: Decodable, Sendable {
    /// The node's kind and the name it is made from, `Text · headline`.
    public let label: String
    public let id: String
    /// The node `add_node` adds, unplaced.
    public let node: JSONValue

    /// What a menu groups it under: its kind, `Text`.
    public var kind: String { label.components(separatedBy: " · ").first ?? label }
    /// What a menu calls it in its kind: the name it is made from, `headline`.
    public var name: String {
        let parts = label.components(separatedBy: " · ")
        return parts.count > 1 ? parts.dropFirst().joined(separator: " · ") : label
    }
}

/// A node a patch adds (PLAN 2.34): its id, its box once placed (`[x, y, width, height]`, canvas
/// units), and the patch: `add_node`, the node entering in the state, then the ops that place it.
public struct Added: Decodable, Sendable {
    public let id: String
    public let cell: [Double]
    public let patch: [JSONValue]
}

/// An icon to insert (PLAN 2.104): its name in its set, its path data (absolute M, L, C, and Z),
/// and the view, `[x, y, width, height]`, it is drawn on.
public struct Icon: Codable, Sendable, Hashable {
    public let name: String
    public let path: String
    public let view: [Double]

    public init(name: String, path: String, view: [Double]) {
        self.name = name
        self.path = path
        self.view = view
    }

    /// Its name as a person reads it, and as its node is named: `airplane-tilt` as Airplane tilt.
    public var said: String {
        let words = name.replacingOccurrences(of: "-", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }

    /// The patch that puts this icon in the place of icon `node` in `state` (PLAN G.23), as the
    /// browser's picker swaps one: its path, its view, and its name, each a `choose` written where
    /// it lives; the node's box, color, and place as they were.
    public func swapping(_ node: String, in state: String) -> [JSONValue] {
        func choose(_ prop: String, _ value: JSONValue) -> JSONValue {
            ["op": "choose", "node": .string(node), "prop": .string(prop), "value": value, "state": .string(state)]
        }
        return [
            choose("path", .string(path)),
            choose("view", .array(view.map { .number($0) })),
            choose("alt", .string(said)),
        ]
    }
}

/// An icon the words found: its name, its tags and categories, and the icon in the weight asked.
public struct IconFound: Decodable, Sendable, Hashable {
    public let name: String
    public let tags: [String]
    public let categories: [String]
    public let icon: Icon
}

/// The icons the words found, best first, at most as many as asked, and how many they found.
public struct IconsFound: Decodable, Sendable {
    public let icons: [IconFound]
    public let found: Int
}

extension ScaenaSession {
    /// The icons that ship (PLAN 2.104), found as `scaena icons` finds them: each word begins a
    /// word of an icon's name, tags, or categories, its name asked whole first; every icon for no
    /// words. `fill` draws them solid, else in outline; at most `limit`.
    public func icons(_ query: String, fill: Bool = false, limit: Int = 240) throws -> IconsFound {
        try call(
            "icons",
            ["query": .string(query), "weight": .string(fill ? "fill" : "regular"), "limit": .number(Double(limit))])
    }

    /// Whether `node`, as `state` shows it, is an icon: a path on a view, as the inspector names it
    /// (PLAN 2.104), which the picker puts another icon in place of (PLAN G.23).
    public func isIcon(_ node: String, in state: String) -> Bool {
        (try? choices(state: state, node: node))?.noun == "Icon"
    }

    /// The patch that inserts `icon` in `state` about `point` (canvas units), as `inserting`
    /// inserts what is offered (PLAN 2.104): a path shape on its view in the theme's accent, as
    /// decoration named by the icon.
    public func insertingIcon(state: String, icon: Icon, at point: CGPoint) throws -> Added {
        let view = JSONValue.array(icon.view.map { .number($0) })
        let given: JSONValue = .object(["name": .string(icon.name), "path": .string(icon.path), "view": view])
        return try call(
            "insertingIcon",
            ["state": .string(state), "icon": given, "x": .number(Double(point.x)), "y": .number(Double(point.y))])
    }

    /// Everything the deck may have inserted (PLAN 2.34): a text in each of the theme's roles,
    /// each shape, each image in the bundle, a chart and a table of each data source, and each
    /// shader preset.
    public func inserts() throws -> [Insert] { try call("inserts") }

    /// The patch that inserts what `inserts` offers `n`th in `state`, about `point` (canvas units),
    /// or in the room nearest it where content there would overlap (PLAN 2.34, 2.79); `named`, a
    /// dropped file's name, names it, and `with`, properties of its own, are set on it: a pasted
    /// sheet's table its columns (PLAN 2.96).
    public func inserting(state: String, n: Int, at point: CGPoint, named: String? = nil, with: JSONValue? = nil) throws
        -> Added
    {
        var args: [String: JSONValue] = [
            "state": .string(state), "n": .number(Double(n)), "x": .number(Double(point.x)), "y": .number(Double(point.y)),
        ]
        if let named { args["named"] = .string(named) }
        if let with { args["with"] = with }
        return try call("inserting", .object(args))
    }

    /// The patch that adds a copy of `node`, with what it holds, beside it in `state` (PLAN 2.34).
    public func duplicating(state: String, node: String) throws -> Added {
        try call("duplicating", ["state": .string(state), "node": .string(node)])
    }

    /// The ops that take `node`, with what it holds, out of `state` and the states after it, and
    /// out of the deck where no state shows it after; or, `everywhere`, out of the deck (PLAN 2.34).
    public func deleting(state: String, node: String, everywhere: Bool = false) throws -> [JSONValue] {
        try call("deleting", ["state": .string(state), "node": .string(node), "everywhere": .bool(everywhere)])
    }
}

/// The patch that locks `nodes` (PLAN 2.95), each by its own `locked`, which holds in every state;
/// or, `on` false, unlocks them. Without `on`, it unlocks where each is locked by its own lock, and
/// locks otherwise, as the browser's Lock does. `own` says whether a node's own lock holds it.
/// Empty where each is so already; `locking` says which it does.
public func locking(_ nodes: [String], own: (String) -> Bool, on: Bool? = nil) -> (ops: [JSONValue], locking: Bool) {
    let locking = on ?? !nodes.allSatisfy(own)
    let path = { (node: String) -> JSONValue in
        let escaped = node.replacingOccurrences(of: "~", with: "~0").replacingOccurrences(of: "/", with: "~1")
        return .string("/nodes/\(escaped)/locked")
    }
    let ops: [JSONValue] =
        locking
        ? nodes.filter { !own($0) }.map { ["op": "add", "path": path($0), "value": true] }
        : nodes.filter { on == false || own($0) }.map { ["op": "remove", "path": path($0)] }
    return (ops, locking)
}

/// The words of a shape to type in (PLAN 2.108): the text that holds them, and the patch that gives
/// the shape one where it holds none, empty where it does.
public struct Worded: Decodable, Sendable {
    public let id: String
    public let patch: [JSONValue]
}

extension ScaenaSession {
    /// The words of `shape` to type in (PLAN 2.108), as the browser's canvas gives them: the text
    /// it holds, or a patch that adds one, empty, in the color that reads on its fill, which goes
    /// where the shape goes.
    public func wording(shape: String) throws -> Worded {
        try call("wording", ["shape": .string(shape)])
    }
}

/// A connector made (PLAN 2.109): its id, and the patch that adds it.
public struct Connected: Decodable, Sendable {
    public let id: String
    public let patch: [JSONValue]
}

extension ScaenaSession {
    /// The patch that joins `from` to `to` in `state` with an arrow that points at `to`, or with a
    /// line (PLAN 2.109), as the browser's canvas makes it: a connector that goes where they go.
    public func connecting(state: String, from: String, to: String, arrow: Bool) throws -> Connected {
        try call("connecting", ["state": .string(state), "from": .string(from), "to": .string(to), "arrow": .bool(arrow)])
    }
}

extension ScaenaSession {
    /// The patch that draws a stroke of the pen in `state` (PLAN 2.114), `points` in canvas units as
    /// the pointer, a finger, or the Pencil gave them, as the browser's canvas draws it: one more
    /// stroke of the drawing `onto` where the state shows it as one, else a new drawing where the
    /// stroke was drawn, smoothed, in the theme's accent, kept to the state's slide.
    public func inking(state: String, points: [CGPoint], onto: String?) throws -> Added {
        let stroke: [JSONValue] = points.map { .array([.number(Double($0.x)), .number(Double($0.y))]) }
        var args: [String: JSONValue] = ["state": .string(state), "points": .array(stroke)]
        if let onto { args["onto"] = .string(onto) }
        return try call("inking", .object(args))
    }
}

/// What a diagram draws a text's lines as (PLAN 2.110): steps joined by arrows, steps around a
/// ring, or a tree by their list levels.
public enum DiagramKind: String, CaseIterable, Sendable {
    case process, cycle, hierarchy

    /// What a menu calls it.
    public var title: String {
        switch self {
        case .process: "Process"
        case .cycle: "Cycle"
        case .hierarchy: "Hierarchy"
        }
    }
}

/// A diagram made from a text's lines (PLAN 2.110): its container, and the patch that makes it in
/// the text's place.
public struct Diagrammed: Decodable, Sendable {
    public let id: String
    public let patch: [JSONValue]
}

extension ScaenaSession {
    /// The patch that draws text `node`'s lines in `state` as a diagram of `kind` in its place
    /// (PLAN 2.110), as the browser's canvas makes it: boxes with their words, joined by
    /// connectors, in a container placed as the text was, the text deleted from the state on.
    public func diagramming(state: String, node: String, kind: DiagramKind) throws -> Diagrammed {
        try call("diagramming", ["state": .string(state), "node": .string(node), "kind": .string(kind.rawValue)])
    }
}
