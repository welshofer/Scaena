import Foundation

/// A state a patch adds (PLAN 2.35): its id, and the patch, one `add_state`.
public struct AddedState: Decodable, Sendable {
    public let id: String
    public let patch: [JSONValue]
}

/// What a state is added as, after the state shown (PLAN 2.35).
public enum StateAdding: String, Sendable {
    /// A step of its slide, right after it, showing what that state shows until it is changed.
    case step
    /// A slide of its own after the slide's last step: empty, in the state's layout.
    case slide
}

extension ScaenaSession {
    /// The patch that adds a state after `state` (PLAN 2.35, 3.14): a step is named after the state
    /// it follows (`cover-2`), a slide `slide`, `slide-2`, …, the first that names no state.
    public func addingState(after state: String, as what: StateAdding) throws -> AddedState {
        try call("addingState", ["state": .string(state), "what": .string(what.rawValue)])
    }
}

/// A slide as the light table shows it (PLAN 2.97): its id, and its states as the timeline plays
/// them, the last of which it is drawn at, as the PDF draws it.
public struct Slide: Identifiable, Equatable, Sendable {
    public let id: String
    public let states: [String]

    /// The state it is drawn at: its last.
    public var last: String { states.last ?? id }

    /// The slides of `slots`, in the order the timeline plays them.
    public static func of(_ slots: [ScaenaSession.Slot]) -> [Slide] {
        var slides: [Slide] = []
        for slot in slots {
            if let latest = slides.last, latest.id == slot.slide {
                slides[slides.count - 1] = Slide(id: latest.id, states: latest.states + [slot.state])
            } else {
                slides.append(Slide(id: slot.slide, states: [slot.state]))
            }
        }
        return slides
    }
}

/// The patches the state list and the light table make (PLAN 2.35, 2.97, 3.14), as the browser's
/// strip and light table make them: each one change, by the user. Moving, copying, and taking
/// out slides keeps what every other state shows (`scaena_core::tracking::keep_looks`).
public enum Restaging {
    /// `state` moved just before `to`, or, not `before`, just after it.
    public static func move(_ state: String, before: Bool, _ to: String) -> [JSONValue] {
        [["op": "move_state", "id": .string(state), (before ? "before" : "after"): .string(to)]]
    }

    /// `state` renamed `to`, its links and the states tracking it renamed with it.
    public static func rename(_ state: String, to: String) -> [JSONValue] {
        [["op": "rename_state", "id": .string(state), "to": .string(to)]]
    }

    public static func remove(_ state: String) -> [JSONValue] {
        [["op": "remove_state", "id": .string(state)]]
    }

    /// `slides`, in the deck's order, moved together just before `to`, or, not `before`, just
    /// after it: the first there, each other just after the one before it.
    public static func moveSlides(_ slides: [String], before: Bool, _ to: String) -> [JSONValue] {
        slides.indices.map { i -> JSONValue in
            guard i > 0 else {
                return ["op": "move_slide", "slide": .string(slides[i]), (before ? "before" : "after"): .string(to)]
            }
            return ["op": "move_slide", "slide": .string(slides[i]), "after": .string(slides[i - 1])]
        }
    }

    /// A copy of each of `slides` just after it; a copy's first state is absolute, so an edit to
    /// either leaves the other.
    public static func duplicateSlides(_ slides: [String]) -> [JSONValue] {
        slides.map { ["op": "duplicate_slide", "slide": .string($0)] }
    }

    public static func removeSlides(_ slides: [String]) -> [JSONValue] {
        slides.map { ["op": "remove_slide", "slide": .string($0)] }
    }
}

/// A rehearsal (PLAN 2.63, 3.14): the deck played as presented, from its first state, with the
/// time each state is shown kept. It keeps no clock: each step is told the time. At the end, a
/// state's cue plays and then its hold, so the hold the time keeps is the time less the cue's
/// span, to a tenth of a second; a state shown twice keeps both times.
public struct Rehearsal: Sendable {
    /// The states as the rehearsal began.
    public let slots: [ScaenaSession.Slot]
    /// The state shown, by its place in `slots`.
    public private(set) var index = 0
    /// How long each state has been shown, ms, less the time since the state shown came up.
    public private(set) var spent: [Double]
    /// Whether it is still running: it stops at the end, or going on from the last state.
    public private(set) var running = true
    public let began: ContinuousClock.Instant
    private var since: ContinuousClock.Instant

    public init(slots: [ScaenaSession.Slot], at now: ContinuousClock.Instant = .now) {
        self.slots = slots
        spent = Array(repeating: 0, count: slots.count)
        began = now
        since = now
        running = !slots.isEmpty
    }

    /// The state shown.
    public var state: String? { slots.indices.contains(index) ? slots[index].state : nil }

    /// Go on (`by` 1) or back (-1), the time since the state shown came up counted toward it.
    /// Going on from the last state stops; going back from the first stays.
    public mutating func go(_ by: Int, at now: ContinuousClock.Instant = .now) {
        guard running else { return }
        count(at: now)
        let next = index + by
        if next >= slots.count {
            running = false
        } else if next >= 0 {
            index = next
        }
    }

    /// Stop, the time since the state shown came up counted toward it.
    public mutating func stop(at now: ContinuousClock.Instant = .now) {
        guard running else { return }
        count(at: now)
        running = false
    }

    /// How long the state shown has been shown, ms.
    public func here(at now: ContinuousClock.Instant = .now) -> Double {
        guard spent.indices.contains(index) else { return 0 }
        return spent[index] + (running ? Self.ms(now - since) : 0)
    }

    /// How long the rehearsal has run, ms.
    public func total(at now: ContinuousClock.Instant = .now) -> Double {
        spent.reduce(0, +) + (running ? Self.ms(now - since) : 0)
    }

    /// Each state as the rehearsal keeps it, in the order it was played.
    public var kept: [Rehearsed] {
        slots.indices.map { i -> Rehearsed in
            let slot = slots[i]
            let keeps: Double? = spent[i] > 0 ? max(0, ((spent[i] - slot.span) / 100).rounded() * 100) : nil
            return Rehearsed(state: slot.state, spent: spent[i], span: slot.span, hold: slot.hold, keeps: keeps)
        }
    }

    /// The patch Keep makes: each state reached holding as long as the time it was shown keeps,
    /// one `set_state` of `hold` each, where that is another hold. A state not reached keeps its own.
    public var holds: [JSONValue] {
        kept.compactMap { row -> JSONValue? in
            guard let keeps = row.keeps, keeps != row.hold else { return nil }
            return ["op": "set_state", "id": .string(row.state), "prop": "hold", "value": .number(keeps)]
        }
    }

    private mutating func count(at now: ContinuousClock.Instant) {
        if spent.indices.contains(index) { spent[index] += Self.ms(now - since) }
        since = now
    }

    static func ms(_ d: Duration) -> Double {
        let (seconds, attoseconds) = d.components
        return Double(seconds) * 1000 + Double(attoseconds) / 1e15
    }
}

/// A state as a rehearsal kept it (PLAN 2.63): how long it was shown, its cue's span, its hold now,
/// and the hold the time keeps; none for a state not reached. Each in ms.
public struct Rehearsed: Equatable, Sendable {
    public let state: String
    public let spent: Double
    public let span: Double
    public let hold: Double
    public let keeps: Double?
}
