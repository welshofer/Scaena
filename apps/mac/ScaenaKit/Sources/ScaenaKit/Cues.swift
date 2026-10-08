import Foundation

extension Cue {
    /// A bar of a state's cue (PLAN 2.44, 3.14), as the browser's cue draws it: the transition, or a
    /// motion, where the engine places it on the cue's clock. Nothing here times anything: a bar
    /// dragged becomes the patch that writes its new time, and the engine places it again.
    public struct Bar: Identifiable, Equatable, Sendable {
        /// Which bar it is, kept across a redraw: `transition`, or the motion's node and kind, counted
        /// where a node has two of a kind.
        public let id: String
        public let label: String
        /// The motion's node and kind; none for the transition.
        public let node: String?
        public let motion: String?
        public let units: Int
        /// When it starts to change and when it rests, ms into the cue.
        public let from: Double
        public let to: Double
        /// Its delay as written, and its duration: each unit's, and for `anim` from its first key to
        /// its last.
        public let delay: Double
        public let duration: Double
        /// On a spring, which lasts as long as it takes to settle: its end does not move.
        public let sprung: Bool

        /// Its start, or its end.
        public enum Part: Sendable {
            case delay, duration
        }

        /// A time moves in whole tens of ms.
        public static let step = 10.0

        /// The patch that moves its start (`delay`) or its end (`duration`) `by` ms in the cue of
        /// `state`, to a whole ten, and what the status says of it: a `time_motion`, written where the
        /// motion is, or for the transition's end a `set_state`. None where nothing moves: the
        /// transition's start, a spring's end, or a time that stays as it is.
        public func timing(_ part: Part, by ms: Double, in state: String) -> (patch: [JSONValue], said: String)? {
            let snap = { (v: Double) -> Double in (v / Self.step).rounded() * Self.step }
            guard part == .delay || !sprung else { return nil }
            guard let node, let motion else {
                let value = max(0, snap(duration + ms))
                guard part == .duration, value != duration else { return nil }
                let op: JSONValue = [
                    "op": "set_state", "id": .string(state), "prop": "transition/duration", "value": .number(value),
                ]
                return ([op], "the transition into \(state) lasts \(Int(value)) ms")
            }
            let was = part == .delay ? delay : duration
            let value = max(part == .duration && motion == "anim" ? Self.step : 0, snap(was + ms))
            guard value != was else { return nil }
            let key = part == .delay ? "delay" : "duration"
            let op: JSONValue = [
                "op": "time_motion", "node": .string(node), "motion": .string(motion), "state": .string(state),
                key: .number(value),
            ]
            let unit = units > 1 ? " a unit" : ""
            guard part == .duration else { return ([op], "\(node)'s \(motion) waits \(Int(value)) ms") }
            return ([op], "\(node)'s \(motion) lasts \(Int(value)) ms\(unit)")
        }
    }
}

extension Cue {
    /// Its bars: the transition, then each motion in the order the state lists them.
    public var bars: [Bar] {
        var seen: [String: Int] = [:]
        let each = motions.map { m -> Bar in
            let name = "\(m.node) \(m.motion)"
            let n = (seen[name] ?? 0) + 1
            seen[name] = n
            let from = m.moving.first ?? m.start
            let to = m.moving.last ?? m.end
            let split = m.split.map { " by \($0)" } ?? ""
            let units = m.units > 1 ? " ×\(m.units)" : ""
            let anim = max(0, to - from - Double(m.units - 1) * m.stagger)
            return Bar(
                id: n == 1 ? name : "\(name) \(n)", label: "\(m.node) · \(m.motion)\(split)\(units)", node: m.node,
                motion: m.motion, units: m.units, from: from, to: to, delay: m.delay,
                duration: m.motion == "anim" ? anim : m.duration, sprung: m.sprung)
        }
        let into = Bar(
            id: "transition", label: "transition", node: nil, motion: nil, units: 1, from: 0, to: transition.duration,
            delay: 0, duration: transition.duration, sprung: transition.sprung)
        return [into] + each
    }
}

/// A motion a cue may add (PLAN 2.44): one of the theme's presets on a node, as it enters, for
/// emphasis, or as it leaves.
public struct MotionOffer: Identifiable, Equatable, Sendable {
    public let node: String
    /// `enter`, `emphasis`, or `exit`.
    public let motion: String
    /// The theme's motion presets, in its order.
    public let presets: [String]

    public var id: String { "\(node) \(motion)" }

    /// What a menu calls it: `title enters`.
    public var label: String {
        switch motion {
        case "enter": "\(node) enters"
        case "exit": "\(node) leaves"
        default: "\(node), for emphasis"
        }
    }

    /// The patch that adds `preset` as this motion in the cue of `state`, `states` the deck's as
    /// the timeline plays them: one `apply_preset`, an exit written in the state the node leaves,
    /// the one before. None where there is none before.
    public func applying(_ preset: String, in state: String, states: [String]) -> [JSONValue]? {
        var written = state
        if motion == "exit" {
            guard let i = states.firstIndex(of: state), i > 0 else { return nil }
            written = states[i - 1]
        }
        let op: JSONValue = [
            "op": "apply_preset", "node": .string(node), "preset": .string(preset), "motion": .string(motion),
            "state": .string(written),
        ]
        return [op]
    }
}

extension ScaenaSession {
    /// What the cue of `state` may add (PLAN 2.44), as the browser's cue offers it: for `node`, the
    /// node selected, as it enters, where it enters there or is a chart, and for emphasis, each
    /// where it has none; and for each node that leaves, as it leaves, where it has no exit. Each
    /// with the theme's motion presets; none where the theme has none.
    public func motionOffers(state: String, node: String?) throws -> [MotionOffer] {
        let inspected: Inspected = try call("inspect", ["state": .string(state)])
        guard let cue = inspected.timeline else { return [] }
        let shown = node.flatMap { inspected.nodes.contains($0) ? $0 : nil }
        // The presets are the theme's: what any node's inspector offers as it enters.
        let chosen = shown.flatMap { try? choices(state: state, node: $0) }
        var presets = Self.presets(chosen)
        for other in inspected.nodes where presets.isEmpty {
            presets = Self.presets(try? choices(state: state, node: other))
        }
        guard !presets.isEmpty else { return [] }
        let has = { (n: String, kind: String) in cue.motions.contains { $0.node == n && $0.motion == kind } }
        var offers: [MotionOffer] = []
        if let shown {
            let enters = inspected.entered.contains(shown) || chosen?.type == "chart"
            if enters && !has(shown, "enter") {
                offers.append(MotionOffer(node: shown, motion: "enter", presets: presets))
            }
            if !has(shown, "emphasis") {
                offers.append(MotionOffer(node: shown, motion: "emphasis", presets: presets))
            }
        }
        for gone in inspected.exited where !has(gone, "exit") {
            offers.append(MotionOffer(node: gone, motion: "exit", presets: presets))
        }
        return offers
    }

    /// The motion presets an inspector offers as a node enters: the theme's.
    private static func presets(_ choices: Choices?) -> [String] {
        let enter = choices?.fields.first { $0.prop == "enter" }
        guard case .name(_, let names, _) = enter?.takes else { return [] }
        return names
    }
}
