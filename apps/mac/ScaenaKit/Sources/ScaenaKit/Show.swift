import Foundation

/// The screen as the presenter turns it (PLAN 2.105): black or white, the slide out of sight.
public enum Blank: String, Sendable {
    case black, white
}

/// A deck played as it is presented (PLAN 3.5), paced as the browser's player paces it (PLAN
/// 2.2, SPEC §2.4): a state of the deck's timeline, and the playhead in its cue. Going on plays
/// the next state's cue, and going on during a cue finishes it; going back shows the state before
/// at rest. A state that holds, short of the last, goes on by itself once its cue and its hold
/// are over; one that does not hold, and the last, comes to rest and waits.
///
/// The presenter's keys (PLAN 2.105), as the browser's player's: the screen black or white, the
/// show waiting where it was, a cue held where it stood, until the slide is back; and a slide's
/// number typed, then Return, going there.
public struct Show: Equatable, Sendable {
    /// The deck's states, as its timeline plays them.
    public let slots: [ScaenaSession.Slot]
    /// The state shown, by its place in `slots`.
    public private(set) var index: Int
    /// Where the canvas is in the state's cue.
    public var playhead: Playhead
    /// The screen black or white, where the presenter turned it so; none while the slide shows.
    public private(set) var blank: Blank?
    /// Where a cue that played stood when the screen went black or white, to play on from.
    private var held: Playhead?
    /// A slide's number as the presenter types it, its digits; empty while none is typed.
    public private(set) var typed = ""

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
    /// hold, short of the last state. None where it waits to be told, or while the screen is black
    /// or white.
    public var holds: Double? {
        guard blank == nil, let slot, slot.hold > 0, index + 1 < slots.count else { return nil }
        return slot.hold
    }

    /// The screen `to`, black or white, a cue that plays held where it stands; the slide again,
    /// where the screen is `to` already (PLAN 2.105).
    public mutating func blanking(_ to: Blank) {
        if blank == to { return unblank() }
        if blank == nil, playhead.playing {
            held = playhead
            playhead.playing = false
        }
        blank = to
    }

    /// The slide again, a cue held under the screen played on from where it stood.
    public mutating func unblank() {
        blank = nil
        if let held { playhead = held }
        held = nil
    }

    /// How many slides the deck has, as a person counts them: a slide's steps are its own.
    public var slides: Int { Set(slots.map(\.slide)).count }

    /// `digit` typed on a slide's number: no slide is numbered 0, nor begins with it.
    public mutating func type(_ digit: Int) {
        guard (0...9).contains(digit) else { return }
        typed = String(String((typed + String(digit)).drop { $0 == "0" }).prefix(4))
    }

    /// The last digit typed taken back; or, with `all`, the number let go.
    public mutating func untype(all: Bool = false) {
        typed = all ? "" : String(typed.dropLast())
    }

    /// The slide typed, its first state's cue played, the slide back where the screen was black or
    /// white: whether the deck has it. The number goes either way.
    @discardableResult
    public mutating func goTyped() -> Bool {
        let number = typed
        typed = ""
        guard let state = slots.state(numbered: number) else { return false }
        blank = nil
        held = nil
        return go(to: state)
    }
}
