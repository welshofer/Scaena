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

extension ScaenaSession {
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
