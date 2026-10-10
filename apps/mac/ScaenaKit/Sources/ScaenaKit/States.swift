import CScaena
import CoreGraphics
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

    /// The patch that starts a slide in `layout`, one of the theme's, after the slide of `state`,
    /// or a blank one with none (PLAN 3.30): named after the layout (`bullets`, `bullets-2`, …),
    /// with a text in each slot that says what goes there, its words to type over.
    public func starting(after state: String, layout: String?) throws -> AddedState {
        var args: [String: JSONValue] = ["state": .string(state)]
        if let layout { args["layout"] = .string(layout) }
        return try call("starting", .object(args))
    }

    /// The slides a person may start after the slide of `state` (PLAN 3.30): one in each of the
    /// theme's layouts, in its order, then a blank one, each painted at rest `height` pixels high.
    /// `starterPicture(i)` takes the `i`th one's picture.
    public func starters(after state: String, height: Int) throws -> [Starter] {
        try call("startersPainted", ["state": .string(state), "height": .number(Double(max(height, 1)))])
    }

    /// The `i`th picture `starters` painted last, taken: a second call throws.
    public func starterPicture(_ i: Int) throws -> Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let painted = scaena_starter_pixels(handle, i, &error)
        let rgba = try Self.take(painted.bytes, error)
        return Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height))
    }

    /// The slides a person may start after the slide of `state`, each with its picture `height`
    /// pixels high. One whose picture cannot be made is left out.
    public func startersDrawn(after state: String, height: Int) throws -> [(starter: Starter, image: CGImage)] {
        try starters(after: state, height: height).enumerated().compactMap { i, starter in
            (try starterPicture(i)).image.map { (starter: starter, image: $0) }
        }
    }
}

/// A slot of the state's layout with nothing placed in it (PLAN 3.30): where the layout says a
/// picture, a figure, or words go, which the canvas outlines with its prompt's words.
public struct WaitingSlot: Decodable, Sendable, Identifiable, Equatable {
    public let slot: String
    /// Its box in the format shown, canvas units.
    public let rect: CGRect
    /// What goes there, as the theme's prompt says it.
    public let words: String
    /// Whether words go there; else a picture or a figure.
    public let typed: Bool
    /// The text role words there are set in, which Insert offers a text of.
    public let role: String?

    public var id: String { slot }

    private enum Keys: String, CodingKey { case slot, rect, words, typed, role }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        slot = try fields.decode(String.self, forKey: .slot)
        let r = try fields.decode([Double].self, forKey: .rect)
        rect = r.count == 4 ? CGRect(x: r[0], y: r[1], width: r[2], height: r[3]) : .null
        words = try fields.decode(String.self, forKey: .words)
        typed = try fields.decode(Bool.self, forKey: .typed)
        role = try fields.decodeIfPresent(String.self, forKey: .role)
    }
}

/// The node a patch puts in a slot that waits for words (PLAN 3.30): its id, and the patch, one
/// `add_node`, then the `hide_node`s that keep it to its slide.
public struct FilledSlot: Decodable, Sendable {
    public let id: String
    public let patch: [JSONValue]
}

extension ScaenaSession {
    /// The slots of `state`'s layout, in the format shown, with nothing placed in them, each
    /// waiting for what its prompt says goes there (PLAN 3.30).
    public func waiting(state: String) throws -> [WaitingSlot] {
        try call("waiting", ["state": .string(state)])
    }

    /// Whether `node`, as `state` shows it, still reads as its slot's prompt (PLAN 3.30): words a
    /// new slide in the layout put there, as they were put, which typing selects whole.
    public func prompted(state: String, node: String) throws -> Bool {
        try call("prompted", ["state": .string(state), "node": .string(node)])
    }

    /// The patch that fills `slot` of `state`'s layout, which waits for words, as a new slide in
    /// the layout fills it (PLAN 3.30): its prompt's words in the slot's role, or a list's items,
    /// named after the slide and the slot (`bullets-header`) and kept to the slide.
    public func filling(state: String, slot: String) throws -> FilledSlot {
        try call("filling", ["state": .string(state), "slot": .string(slot)])
    }
}

/// A slide a person may start (PLAN 3.30): one in a layout of the theme, a text in each slot that
/// says what goes there, or a blank one; and its picture's size, pixels.
public struct Starter: Decodable, Sendable, Identifiable, Equatable {
    /// The theme's layout it is in; none for the blank slide.
    public let layout: String?
    /// What the theme says the layout is for.
    public let description: String?
    /// The gallery's section it is offered under (`Titles`, `Words`, `Pictures`); none for the
    /// blank slide, which comes last.
    public let group: String?
    public let width: Int
    public let height: Int

    public var id: String { layout ?? "" }
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

extension Array where Element == ScaenaSession.Slot {
    /// The state a slide's number names as a person types it, the window's own numbers: "3" or
    /// "Slide 3" its first state, "Slide 3, step 2" its second; none where the deck has no such
    /// slide or step, or the words are no number. What ⌘K links words to (PLAN 2.70).
    public func state(numbered words: String) -> String? {
        var said = words.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
        if said.hasPrefix("slide") { said.removeFirst("slide".count) }
        let parts = said.components(separatedBy: "step")
        let trimmed = CharacterSet.whitespaces.union(CharacterSet(charactersIn: ","))
        guard (1...2).contains(parts.count), let number = Int(parts[0].trimmingCharacters(in: trimmed)),
            let step = parts.count == 2 ? Int(parts[1].trimmingCharacters(in: trimmed)) : 1, number >= 1, step >= 1
        else { return nil }
        // Numbered as the slide list numbers them: a slide's steps follow it.
        var slide: String?
        var counted = 0
        var stepped = 0
        for slot in self {
            if slot.slide != slide {
                slide = slot.slide
                counted += 1
                stepped = 0
            }
            stepped += 1
            if counted == number, stepped == step { return slot.state }
            if counted > number { break }
        }
        return nil
    }
}
