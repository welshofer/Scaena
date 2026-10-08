import Foundation

/// A deck played as it is presented (PLAN 3.5), paced as the browser's player paces it (PLAN
/// 2.2, SPEC §2.4): a state of the deck's timeline, and the playhead in its cue. Going on plays
/// the next state's cue, and going on during a cue finishes it; going back shows the state before
/// at rest. A state that holds, short of the last, goes on by itself once its cue and its hold
/// are over; one that does not hold, and the last, comes to rest and waits.
public struct Show: Equatable, Sendable {
    /// The deck's states, as its timeline plays them.
    public let slots: [ScaenaSession.Slot]
    /// The state shown, by its place in `slots`.
    public private(set) var index: Int
    /// Where the canvas is in the state's cue.
    public var playhead: Playhead

    /// The deck played from `state`, its cue first; from the first state where none is named.
    public init(slots: [ScaenaSession.Slot], from state: String? = nil) {
        self.slots = slots
        index = state.flatMap { state in slots.firstIndex { $0.state == state } } ?? 0
        playhead = Playhead()
    }

    /// The state shown.
    public var slot: ScaenaSession.Slot? { slots.indices.contains(index) ? slots[index] : nil }

    /// The state that comes next, if any.
    public var next: ScaenaSession.Slot? { slots.indices.contains(index + 1) ? slots[index + 1] : nil }

    /// Go on: finish the cue that plays, or play the next state's.
    public mutating func on() {
        if playhead.playing {
            playhead = .rest
        } else if index + 1 < slots.count {
            index += 1
            playhead = Playhead()
        }
    }

    /// Go back: the state before, at rest.
    public mutating func back() {
        index = max(index - 1, 0)
        playhead = .rest
    }

    /// `state`, its cue played: where a link goes (PLAN 2.70). False for a state the deck lacks.
    @discardableResult
    public mutating func go(to state: String) -> Bool {
        guard let at = slots.firstIndex(where: { $0.state == state }) else { return false }
        index = at
        playhead = Playhead()
        return true
    }

    /// The first state, its cue played.
    public mutating func first() {
        index = 0
        playhead = Playhead()
    }

    /// The last state, at rest.
    public mutating func last() {
        index = max(slots.count - 1, 0)
        playhead = .rest
    }

    /// How long the state shown stays at rest before it gives way to the next by itself, ms: its
    /// hold, short of the last state. None where it waits to be told.
    public var holds: Double? {
        guard let slot, slot.hold > 0, index + 1 < slots.count else { return nil }
        return slot.hold
    }
}
