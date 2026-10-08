import CoreGraphics
import Foundation

/// Several nodes arranged (PLAN 2.42): where each lands, the patch that puts them all there, and,
/// moved together, the guides the box around them meets (PLAN 2.57).
public struct Arranged: Decodable, Sendable {
    public let landed: [Landed]
    public let patch: [JSONValue]
    public let guides: [[CGPoint]]

    /// Where a node lands: its box there, canvas units.
    public struct Landed: Decodable, Sendable {
        public let node: String
        public let cell: CGRect

        private enum Keys: String, CodingKey { case node, cell }

        public init(from decoder: any Decoder) throws {
            let fields = try decoder.container(keyedBy: Keys.self)
            node = try fields.decode(String.self, forKey: .node)
            cell = try rect(try fields.decode([Double].self, forKey: .cell))
        }
    }

    private enum Keys: String, CodingKey { case landed, patch, guides }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        landed = try fields.decode([Landed].self, forKey: .landed)
        patch = try fields.decode([JSONValue].self, forKey: .patch)
        let lines = try fields.decodeIfPresent([[Double]].self, forKey: .guides) ?? []
        guides = lines.filter { $0.count == 4 }.map { [CGPoint(x: $0[0], y: $0[1]), CGPoint(x: $0[2], y: $0[3])] }
    }
}

/// A new group for nodes (PLAN 2.43): its id, and the patch that makes it.
public struct Grouping: Decodable, Sendable {
    public let id: String
    public let patch: [JSONValue]
}

/// How several nodes are arranged (PLAN 2.42), as the inspector's buttons ask.
public enum Arrangement: Sendable, Equatable {
    /// Each on the edge, or the middle, of the box around them: `left`, `center`, `right`, `top`,
    /// `middle`, or `bottom`.
    case align(String)
    /// Each the same gap from the next, `across` or `down`.
    case spread(String)
    /// In front, or behind, among what their container paints: `forward`, `backward`, `front`,
    /// or `back`.
    case order(String)

    /// As the session asks it.
    var asked: JSONValue {
        switch self {
        case .align(let edge): ["align": .string(edge)]
        case .spread(let way): ["spread": .string(way)]
        case .order(let how): ["order": .string(how)]
        }
    }
}

extension ScaenaSession {
    /// `nodes`, children of one container, moved together `by` (canvas units) in `state`, the
    /// first snapped as a drag of it alone snaps, `free` off the grid, the rest as far as it went
    /// (PLAN 2.42): where each lands, the patch, and the guides the box around them meets within
    /// `reach`; kept to `state` with `fork`. None where nothing moves them so.
    public func together(
        state: String, nodes: [String], by: CGVector, free: Bool = false, fork: Bool = false, reach: Double = 0
    ) throws -> Arranged? {
        try call(
            "together",
            [
                "state": .string(state), "nodes": .array(nodes.map { .string($0) }), "dx": .number(Double(by.dx)),
                "dy": .number(Double(by.dy)), "free": .bool(free), "fork": .bool(fork), "reach": .number(reach),
            ])
    }

    /// `nodes`, children of one container, arranged `how` in `state` (PLAN 2.42): one patch; none
    /// where each is so already, or nothing arranges them so.
    public func arranging(state: String, nodes: [String], how: Arrangement, fork: Bool = false) throws -> Arranged? {
        try call(
            "arranging",
            ["state": .string(state), "nodes": .array(nodes.map { .string($0) }), "how": how.asked, "fork": .bool(fork)])
    }

    /// The patch that puts `nodes`, children of one container, in a new group where they stand in
    /// `state` (PLAN 2.43).
    public func grouping(state: String, nodes: [String]) throws -> Grouping {
        try call("grouping", ["state": .string(state), "nodes": .array(nodes.map { .string($0) })])
    }
}

extension Field {
    init(prop: String, takes: Takes, value: JSONValue?, lives: Lives?, literal: Bool) {
        self.prop = prop
        self.takes = takes
        self.value = value
        self.lives = lives
        self.literal = literal
    }

    /// What every one of several nodes' inspectors offers (PLAN 2.42), as the browser's inspector
    /// offers what they share: each field the first offers that every other offers too, with its
    /// value where they all show it alike, and where it lives where they agree.
    public static func shared(_ all: [Choices]) -> [Field] {
        guard let first = all.first else { return [] }
        let rest = all.dropFirst()
        return first.fields.compactMap { field in
            let others = rest.compactMap { choices in choices.fields.first { $0.prop == field.prop } }
            guard others.count == rest.count else { return nil }
            let alike = others.allSatisfy { $0.value == field.value }
            let lives = others.allSatisfy { $0.lives == field.lives } ? field.lives : nil
            return Field(
                prop: field.prop, takes: field.takes, value: alike ? field.value : nil, lives: lives,
                literal: alike && field.literal)
        }
    }
}
