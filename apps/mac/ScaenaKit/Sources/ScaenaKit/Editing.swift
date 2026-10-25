import CScaena
import CoreGraphics
import Foundation

// What the session says about a deck being edited (PLAN 2.3, 3.4), typed as the browser's editor
// reads it: what compiling and lint found, what an inspector offers, the layers, the boxes a
// pointer selects, and a state's cue. Each is the JSON `Player` gives a page, decoded.

/// How grave a finding is (SPEC §7.5).
public enum Severity: String, Decodable, Sendable, Comparable {
    case info, warning, error

    public static func < (a: Severity, b: Severity) -> Bool { a.rank < b.rank }

    private var rank: Int {
        switch self {
        case .info: 0
        case .warning: 1
        case .error: 2
        }
    }
}

/// Where something is in the source: UTF-16 offsets, and the line and column it starts at.
public struct Place: Decodable, Equatable, Sendable {
    public let from: Int
    public let to: Int
    public let line: Int
    public let col: Int
}

/// What compiling or lint found (SPEC §7.5): where it is in the source, the state and node it
/// is about, and the fix that resolves it, where one is safe.
public struct Finding: Decodable, Equatable, Sendable {
    public let code: String
    public let severity: Severity
    public let message: String
    /// What it finds, in a sentence a person would say (PLAN 2.99, `scaena_core::words::issue`),
    /// as the window shows it; none for a finding with no sentence of its own.
    public let said: String?
    /// A JSON pointer into `deck.json`.
    public let path: String?
    public let state: String?
    public let node: String?
    /// The format it holds in, where it is not the deck's own canvas.
    public let format: String?
    public let hint: String?
    /// A JSON Patch that resolves it: safe, and never the deck's content.
    public let fix: [JSONValue]?
    public let at: Place?
    /// Whether its fix applies to the source compiled last.
    public let fixable: Bool
    /// Whether it holds in the format frames are laid out in.
    public let shown: Bool
    /// The formats it holds in: `""` for the deck's own canvas, then each of the deck's `formats`
    /// (PLAN 2.62). What the formats side by side count.
    public let formats: [String]?
}

/// What compiling a source says (SPEC §4): why it does not compile, or what validation finds in
/// the deck it says, where each state starts, and whether the deck validated, and so is what
/// frames show from now on.
public struct Compiling: Decodable, Sendable {
    public let error: Finding?
    public let findings: [Finding]
    public let states: [StateLine]
    public let valid: Bool
}

/// A state, and where the line that declares it starts in the source (UTF-16).
public struct StateLine: Decodable, Equatable, Sendable {
    public let state: String
    public let offset: Int

    public init(from decoder: any Decoder) throws {
        var pair = try decoder.unkeyedContainer()
        state = try pair.decode(String.self)
        offset = try pair.decode(Int.self)
    }
}

/// What lint finds in the deck compiled last, in its own format and each of its `formats`.
public struct Linting: Decodable, Sendable {
    public let findings: [Finding]
    /// Whether the layout rules ran: they run once nothing above them is an error.
    public let laid: Bool
    /// Whether they ran on every state, or on one, the others' findings kept from the last time.
    public let whole: Bool
}

/// The deck a session compiled last, taken to be laid out on every state for lint on a thread of
/// its own (PLAN 4.13): it shares the bundle's files with the session, and builds an engine of its
/// own, so laying it out touches nothing the session's thread uses. Sendable: it is taken on the
/// session's thread, laid out on one other, and handed back after, one thread at a time.
public final class Laying: @unchecked Sendable {
    let handle: OpaquePointer

    init(handle: OpaquePointer) {
        self.handle = handle
    }

    deinit {
        scaena_laying_free(handle)
    }

    /// Lay the deck out on every state and run lint's layout rules on it: the slow part of a lint
    /// of every state, on the thread that calls it. Once.
    public func lay() {
        _ = scaena_laying_lay(handle)
    }

    /// The deck's narrative health (PLAN G.9), as the session's `health()` reads it, read on the
    /// thread that calls it, laying nothing out (lift-up narrative PERF-1).
    public func health() throws -> Health {
        try ScaenaSession.decode(scaena_laying_health(handle), as: Health.self)
    }

    /// `state` at rest in `format`, or on the deck's own canvas where none, painted by the CPU
    /// painter `width` pixels wide: the session's pixels at rest, drawn on the thread that calls
    /// it, by an engine built at the first and kept for the next (lift-up PERF-2).
    public func pixels(format: String?, state: String, width: Int) throws -> ScaenaSession.Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let wide = UInt32(max(width, 1))
        // A format by its name, or none: the C string made only where there is one.
        let painted: ScaenaPixels
        if let format {
            painted = scaena_laying_pixels(handle, format, state, wide, &error)
        } else {
            painted = scaena_laying_pixels(handle, nil, state, wide, &error)
        }
        let rgba = try ScaenaSession.take(painted.bytes, error)
        return ScaenaSession.Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height))
    }

    /// The cue into `state` in `format`, or on the deck's own canvas where none, laid out ahead as
    /// the session's `prepare` lays it out, on the thread that calls it: a show's next cue laid out
    /// while the state shown rests, away from the window, then handed to the session's
    /// `cue(laid:)` (lift-up 10-25b PERF-5).
    public func cue(format: String?, state: String) throws -> Cue {
        var error: UnsafeMutablePointer<CChar>?
        // A format by its name, or none: the C string made only where there is one.
        let laid: OpaquePointer?
        if let format {
            laid = scaena_laying_cue(handle, format, state, &error)
        } else {
            laid = scaena_laying_cue(handle, nil, state, &error)
        }
        guard let laid else { throw ScaenaError.taking(error) }
        return Cue(handle: laid)
    }
}

/// A cue laid out ahead on a `Laying`, away from the session, until the session takes it (lift-up
/// 10-25b PERF-5). Sendable: it is laid out on one thread and handed to the session's after.
public final class Cue: @unchecked Sendable {
    let handle: OpaquePointer

    init(handle: OpaquePointer) {
        self.handle = handle
    }

    deinit {
        scaena_cue_free(handle)
    }
}

/// What an inspector offers (PLAN 2.33, 2.36): each property of a node in a state, or of the
/// state itself, with what it takes, the value shown, and where that value lives.
public struct Choices: Decodable, Sendable {
    /// The node, or none for the state's own.
    public let node: String?
    public let type: String?
    public let state: String
    public let fields: [Field]
    /// The slide's number and footer (ADR-0028): offered with the state's own fields.
    public let furniture: FurnitureChoice?
    /// The node's kind as a person names it: `Text`, `Chart`, a shape by its kind (`Rectangle`, a
    /// `Star`), a container with a panel of its own a `Card`; `Icon` for a filled path on a view,
    /// which the icon picker replaces (PLAN G.23), and a pen's drawing, stroked, a `Drawing`.
    public let noun: String?
    /// Where a fetched picture came from, as the Format tab's Map section heads it (ADR-0030): "Map
    /// from Geoapify, fetched 15 October 2026".
    public let fetched: String?
    /// A picture's frame, said, where its theme frames pictures (PLAN T.3a): the theme's, which
    /// nothing on the picture changes, so the inspector shows it and offers nothing.
    public let framed: String?

    /// Whether the writing tools have words to write for what is offered (PLAN 2.101): a text's, a
    /// picture's description, or the slide's notes. A shape, a chart, a table, or a group has none,
    /// and the inspector offers it no writing, as the browser's offers none.
    public var writes: Bool { node == nil || type == "text" || type == "image" }
}

/// What an inspector offers of a slide's number and footer (ADR-0028): the deck's, which
/// `furnish` sets, and the slide's own, which `set_state`'s `furniture` writes on its first state.
public struct FurnitureChoice: Decodable, Sendable, Equatable {
    /// Whether the theme places a number or a footer at all.
    public let placed: Bool
    /// Whether the deck numbers its slides.
    public let number: Bool
    /// The words at the foot of its slides.
    public let footer: String?
    /// The slide's first state, where the slide says whether it shows them.
    public let head: String
    /// What the slide says: shown, hidden, or as its layout has them (none).
    public let slide: Bool?
    /// Whether the slide shows them, as it stands.
    public let shown: Bool

    /// The patch that numbers the deck's slides, or not.
    public static func numbering(_ on: Bool) -> [JSONValue] { [["op": "furnish", "number": .bool(on)]] }

    /// The patch that sets the deck's footer, or takes it away with no words.
    public static func footing(_ words: String) -> [JSONValue] {
        let trimmed = words.trimmingCharacters(in: .whitespacesAndNewlines)
        return [["op": "furnish", "footer": trimmed.isEmpty ? .null : .string(trimmed)]]
    }

    /// The patch that sets the deck's footer to `words`, as `footing` does, or none where the words,
    /// trimmed, are the footer it has: a field left with a space added changes nothing.
    public func footing(to words: String) -> [JSONValue]? {
        let trimmed = words.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed == (footer ?? "") ? nil : FurnitureChoice.footing(trimmed)
    }

    /// The patch that shows the number and footer on this slide, hides them, or leaves them as its
    /// layout has them (`nil`): on the slide's first state.
    public func showing(_ shows: Bool?) -> [JSONValue] {
        [["op": "set_state", "id": .string(head), "prop": "furniture", "value": shows.map(JSONValue.bool) ?? .null]]
    }
}

/// One property an inspector edits.
public struct Field: Decodable, Sendable, Identifiable {
    /// A property, or one key of an object property (`style/color`): what `choose` names.
    public let prop: String
    public let takes: Takes
    /// The value the state shows, as the deck sets it; none where the theme's shows.
    public let value: JSONValue?
    /// Where that value lives: where a choice is written.
    public let lives: Lives?
    /// The value is written out where the theme has names: an override.
    public let literal: Bool
    /// What the theme gives a text's look where the deck sets none, which the slide then shows:
    /// its role's family, weight, size, italic, case, tracking, or color. None for the rest.
    public let theme: JSONValue?
    /// Each value it offers as the engine says it (`words.said`): a language as a person calls it,
    /// `cpp` as C++ (PLAN 2.133).
    public let said: [String: String]

    public var id: String { prop }

    private enum Keys: String, CodingKey { case prop, takes, value, lives, literal, theme, words }
    private struct Worded: Decodable { let said: [String: String]? }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        prop = try fields.decode(String.self, forKey: .prop)
        takes = try fields.decode(Takes.self, forKey: .takes)
        value = try fields.decodeIfPresent(JSONValue.self, forKey: .value)
        lives = try fields.decodeIfPresent(Lives.self, forKey: .lives)
        literal = try fields.decodeIfPresent(Bool.self, forKey: .literal) ?? false
        theme = try fields.decodeIfPresent(JSONValue.self, forKey: .theme)
        said = (try? fields.decodeIfPresent(Worded.self, forKey: .words))??.said ?? [:]
    }
}

/// What a property takes.
public enum Takes: Decodable, Sendable {
    /// One of the theme's names of a kind (`of`), in its order; with `overrides`, a value written
    /// out too, which goes in the deck's `overrides`.
    case name(of: String, names: [String], overrides: Bool)
    /// One of these words.
    case word([String])
    /// A number: at least `min`, or more than `above`; at most `max`; whole with `whole`.
    case number(min: Double?, above: Double?, max: Double?, whole: Bool)
    case flag
    /// Words for people, as they are written: a state's notes.
    case text
    /// Fractions of an image, from 0 to 1, one for each name.
    case fractions([String])
    /// A kind this build does not know: shown, not edited.
    case other(String)

    private enum Keys: String, CodingKey { case kind, of, names, overrides, words, min, above, max, whole }

    public init(from decoder: any Decoder) throws {
        let takes = try decoder.container(keyedBy: Keys.self)
        let kind = try takes.decode(String.self, forKey: .kind)
        switch kind {
        case "name":
            let overrides = try takes.decodeIfPresent(Bool.self, forKey: .overrides) ?? false
            self = try .name(
                of: takes.decode(String.self, forKey: .of), names: takes.decode([String].self, forKey: .names),
                overrides: overrides)
        case "word":
            self = try .word(takes.decode([String].self, forKey: .words))
        case "number":
            let whole = try takes.decodeIfPresent(Bool.self, forKey: .whole) ?? false
            self = try .number(
                min: takes.decodeIfPresent(Double.self, forKey: .min),
                above: takes.decodeIfPresent(Double.self, forKey: .above),
                max: takes.decodeIfPresent(Double.self, forKey: .max), whole: whole)
        case "flag":
            self = .flag
        case "text":
            self = .text
        case "fractions":
            self = try .fractions(takes.decode([String].self, forKey: .names))
        default:
            self = .other(kind)
        }
    }
}

/// Where a value a state shows lives: what a choice changes.
public enum Lives: Decodable, Equatable, Sendable {
    /// The deck's `overrides`, in every state: an override.
    case overrides
    /// A state's delta, by the state's id.
    case state(String)
    /// The node's own properties.
    case node

    private enum Keys: String, CodingKey { case state }

    public init(from decoder: any Decoder) throws {
        if let word = try? decoder.singleValueContainer().decode(String.self) {
            self = word == "overrides" ? .overrides : .node
        } else {
            self = try .state(decoder.container(keyedBy: Keys.self).decode(String.self, forKey: .state))
        }
    }
}

/// One node of a state's layers (PLAN 2.95), the topmost first, with what it holds.
public struct Layer: Decodable, Sendable, Identifiable {
    public let node: String
    public let type: String
    /// Whether the state shows it.
    public let shown: Bool
    /// Whether it is locked: the canvas passes over it.
    public let locked: Bool
    public let children: [Layer]

    public var id: String { node }

    private enum Keys: String, CodingKey { case node, type, shown, locked, children }

    public init(from decoder: any Decoder) throws {
        let layer = try decoder.container(keyedBy: Keys.self)
        node = try layer.decode(String.self, forKey: .node)
        type = try layer.decode(String.self, forKey: .type)
        shown = try layer.decode(Bool.self, forKey: .shown)
        locked = try layer.decodeIfPresent(Bool.self, forKey: .locked) ?? false
        children = try layer.decodeIfPresent([Layer].self, forKey: .children) ?? []
    }
}

/// A node's box at rest (ADR-0013), canvas units, and where its `transform` draws it.
public struct NodeBox: Decodable, Sendable {
    public let node: String
    /// `[x, y, width, height]`.
    public let rect: [Double]
    /// The container or group it sits in.
    public let parent: String?
    /// Whether it draws: a container with no panel, and a group, only hold others.
    public let draws: Bool
    /// The map `[a, b, c, d, e, f]` from its box as laid out to where it is drawn
    /// (`x' = a·x + c·y + e`), where something moves it.
    public let transform: [Double]?
    /// The node whose lock holds it, itself or what holds it (PLAN 2.95).
    public let locked: String?
    /// A shape that takes words (PLAN 2.108): any but a line or an arrow.
    public let takesWords: Bool?
    /// The text that holds a shape's words (PLAN 2.108), which a click on them reaches as the
    /// shape and a double click types in.
    public let words: String?
    /// A connector's ends, from and to (PLAN 2.109), which place it: the canvas moves, resizes,
    /// and turns nothing of it.
    public let joins: [String]?
    /// A connector's line, its points in canvas units (PLAN 2.109), which a pointer picks it by:
    /// its box is mostly empty.
    public let route: [[Double]]?
    /// A code text's language (PLAN 2.133), which its chip shows.
    public let code: CodeLanguage?

    /// A code text's language: as the deck names it (`cpp`), and as a person calls it (`C++`).
    public struct CodeLanguage: Decodable, Sendable, Equatable {
        public let language: String
        public let said: String
    }

    /// Its box's corners where it is drawn, clockwise from the top left, canvas units.
    public var corners: [CGPoint] { drawn(rect, through: transform) }

    /// The box about its corners where it is drawn, canvas units: what shows it whole, as the view
    /// is moved to bring it into view (PLAN 4.12). None where it has no box.
    public var extent: CGRect {
        let c = corners
        guard let x0 = c.map(\.x).min(), let x1 = c.map(\.x).max(), let y0 = c.map(\.y).min(),
            let y1 = c.map(\.y).max()
        else { return .null }
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }

    /// Whether `point`, canvas units, is inside its box where it is drawn: within the four corners
    /// its map draws it at, its edges included. A connector holds what stands within 12 units of
    /// its line, as the engine's hit takes it (PLAN 2.109).
    public func holds(_ point: CGPoint) -> Bool {
        if let route {
            let points = route.compactMap { $0.count == 2 ? CGPoint(x: $0[0], y: $0[1]) : nil }
            return zip(points, points.dropFirst()).contains { a, b in
                let (dx, dy) = (b.x - a.x, b.y - a.y)
                let length = dx * dx + dy * dy
                let t = length > 0 ? min(1, max(0, ((point.x - a.x) * dx + (point.y - a.y) * dy) / length)) : 0
                return hypot(a.x + t * dx - point.x, a.y + t * dy - point.y) <= 12
            }
        }
        let c = corners
        guard c.count == 4 else { return false }
        var turn = 0.0
        for i in 0..<4 {
            let (a, b) = (c[i], c[(i + 1) % 4])
            let cross = Double((b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x))
            guard cross != 0 else { continue }
            if turn == 0 { turn = cross } else if (cross > 0) != (turn > 0) { return false }
        }
        return turn != 0
    }
}

extension [NodeBox] {
    /// What a click at `point` (canvas units) would select, read from the boxes alone, as the
    /// browser's canvas shows it under the pointer (PLAN 2.29, 4.5): the topmost that draws there
    /// and no lock holds, a shape's words reached as the shape (PLAN 2.108). Nothing is asked of
    /// the engine.
    public func under(_ point: CGPoint) -> String? {
        guard let top = last(where: { $0.draws && $0.locked == nil && $0.holds(point) }) else { return nil }
        return reached(top.node)
    }

    /// What a press on `node` reaches (PLAN 2.108): a shape's words, reached as the shape, which a
    /// click selects, a drag moves, and a right click or a long press offers what is done to, as
    /// the browser's canvas reaches them; anything else as itself.
    public func reached(_ node: String) -> String {
        guard let parent = first(where: { $0.node == node })?.parent, first(where: { $0.node == parent })?.words == node
        else { return node }
        return parent
    }
}

/// A node that draws at a point (ADR-0013), with the containers it sits in, innermost first.
public struct Hit: Decodable, Sendable {
    public let node: String
    public let rect: [Double]
    public let containers: [String]
    public let transform: [Double]?
    /// The node whose lock holds it, which a pointer passes over (PLAN 2.95).
    public let locked: String?
}

/// `rect`'s corners through `transform`, clockwise from the top left.
func drawn(_ rect: [Double], through transform: [Double]?) -> [CGPoint] {
    guard rect.count == 4 else { return [] }
    let (x, y, w, h) = (rect[0], rect[1], rect[2], rect[3])
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
    guard let m = transform, m.count == 6 else { return corners.map { CGPoint(x: $0.0, y: $0.1) } }
    return corners.map { CGPoint(x: m[0] * $0.0 + m[2] * $0.1 + m[4], y: m[1] * $0.0 + m[3] * $0.1 + m[5]) }
}

/// A state's cue, as `scaena inspect --timeline` places it (PLAN 1.14, 2.44): its transition
/// and each motion, ms from when the transition into it starts.
public struct Cue: Decodable, Sendable {
    /// When it starts on the deck's timeline.
    public let start: Double
    /// Its transition and motions, from `start`.
    public let span: Double
    /// Its dwell at rest before the next state starts.
    public let hold: Double
    public let transition: Transition
    public let motions: [Motion]

    public struct Transition: Decodable, Sendable {
        public let duration: Double
        /// On a spring, which lasts as long as it takes to settle (SPEC §3.9).
        public let sprung: Bool
        /// A slide transition's style (PLAN 2.111), which moves the slide left whole: none of its
        /// exits plays.
        public let style: String?

        private enum Keys: String, CodingKey { case duration, curve, style }

        public init(from decoder: any Decoder) throws {
            let fields = try decoder.container(keyedBy: Keys.self)
            duration = try fields.decode(Double.self, forKey: .duration)
            sprung = try fields.decodeIfPresent(JSONValue.self, forKey: .curve)?["spring"] != nil
            style = try fields.decodeIfPresent(String.self, forKey: .style)
        }
    }

    public struct Motion: Decodable, Sendable {
        public let node: String
        /// `enter`, `exit`, `emphasis`, or `anim`.
        public let motion: String
        /// What it moves one at a time: lines, words, glyphs, children, or marks.
        public let split: String?
        /// How many it moves, each `stagger` ms after the one before.
        public let units: Int
        public let stagger: Double
        public let start: Double
        public let end: Double
        /// When its first unit starts to change and its last comes to rest.
        public let moving: [Double]
        /// Its `delay` as written, and each unit's `duration`: what `time_motion` reads and sets.
        public let delay: Double
        public let duration: Double
        /// Where it is written: a JSON pointer into the deck.
        public let written: String?
        /// On a spring, which lasts as long as it takes to settle.
        public let sprung: Bool

        private enum Keys: String, CodingKey {
            case node, motion, split, units, stagger, start, end, moving, delay, duration, written, curve
        }

        public init(from decoder: any Decoder) throws {
            let fields = try decoder.container(keyedBy: Keys.self)
            node = try fields.decode(String.self, forKey: .node)
            motion = try fields.decode(String.self, forKey: .motion)
            split = try fields.decodeIfPresent(String.self, forKey: .split)
            units = try fields.decodeIfPresent(Int.self, forKey: .units) ?? 1
            stagger = try fields.decodeIfPresent(Double.self, forKey: .stagger) ?? 0
            start = try fields.decode(Double.self, forKey: .start)
            end = try fields.decode(Double.self, forKey: .end)
            moving = try fields.decode([Double].self, forKey: .moving)
            delay = try fields.decodeIfPresent(Double.self, forKey: .delay) ?? 0
            duration = try fields.decodeIfPresent(Double.self, forKey: .duration) ?? end - start
            written = try fields.decodeIfPresent(String.self, forKey: .written)
            sprung = try fields.decodeIfPresent(JSONValue.self, forKey: .curve)?["spring"] != nil
        }
    }
}

extension ScaenaSession {
    /// Compile `source` (SPEC §4). A deck that validates is what frames show from then on.
    public func compile(_ source: String) throws -> Compiling { try call("compile", ["source": .string(source)]) }

    /// Lint the deck compiled last: every state, or with `state` the layout rules on that state
    /// alone, the others' findings kept from the last time they ran on every state. Dates are
    /// judged at `now`, the Mac's clock (W428, ADR-0030).
    public func lint(state: String? = nil, now: Date = Date()) throws -> Linting {
        var args: [String: JSONValue] = ["now": .string(now.formatted(.iso8601))]
        if let state { args["state"] = .string(state) }
        return try call("lint", .object(args))
    }

    /// The deck compiled last, taken to be linted on every state away from the session (PLAN
    /// 4.13): laid out on another thread while the session answers on its own, then handed back
    /// to `lint(laid:)`. None where no source compiled to a deck that validates.
    public func laying() -> Laying? {
        scaena_laying_new(handle).map(Laying.init)
    }

    /// Whether `laying` was taken from the deck as it stands: not once another source compiled, a
    /// file was handed over, or the theme was edited, when what it draws is an older deck's.
    public func holds(_ laying: Laying) -> Bool {
        scaena_laying_holds(handle, laying.handle)
    }

    /// Take `laid`, a cue laid out ahead away from the session (`Laying.cue`), as the one the next
    /// frames into its state sample, laying nothing out: false where the deck shown or its format
    /// changed since, or that cue is kept already (lift-up 10-25b PERF-5).
    @discardableResult
    public func cue(laid: Cue) -> Bool {
        scaena_cue_laid(handle, laid.handle)
    }

    /// What lint finds on every state, as `lint()` says it, from what `laid` found once laid out,
    /// laying nothing out here: none where the deck changed after it was taken (another source
    /// compiled, a file handed over, the theme edited), so an older deck's findings never stand
    /// for this one's, or where it was not laid out. Dates are judged at `now`, as `lint()` judges
    /// them.
    public func lint(laid: Laying, now: Date = Date()) throws -> Linting? {
        try Self.decode(scaena_lint_laid(handle, laid.handle, now.formatted(.iso8601)), as: Laid.self).linting
    }

    /// What `scaena_lint_laid` answers: the linting, or none where it is stale.
    private struct Laid: Decodable {
        let linting: Linting?
    }

    /// The source compiled last with `patch`, a finding's fix, applied: the fixed deck as `.scn`.
    public func fix(_ patch: [JSONValue]) throws -> String { try call("fix", ["patch": .array(patch)]) }

    /// What an inspector offers for `node` as `state` shows it (PLAN 2.33).
    public func choices(state: String, node: String) throws -> Choices {
        try call("choices", ["state": .string(state), "node": .string(node)])
    }

    /// What an inspector offers for `state` itself (PLAN 2.36): its layout, its transition, its
    /// hold, and its notes.
    public func stateChoices(state: String) throws -> Choices {
        try call("stateChoices", ["state": .string(state)])
    }

    /// `state`'s layers, the topmost first (PLAN 2.95).
    public func layers(state: String) throws -> [Layer] { try call("layers", ["state": .string(state)]) }

    /// Each visible node's box in `state` at rest (ADR-0013): those that draw, in paint order,
    /// then the containers and groups that only hold others.
    public func boxes(state: String) throws -> [NodeBox] { try call("boxes", ["state": .string(state)]) }

    /// The nodes that draw at `point` (canvas units) in `state` at rest, topmost first.
    public func hits(state: String, at point: CGPoint) throws -> [Hit] {
        try call("hit", ["state": .string(state), "x": .number(Double(point.x)), "y": .number(Double(point.y))])
    }

    /// `state`'s cue: its transition and each motion, where the timeline places them.
    public func cue(state: String) throws -> Cue {
        let inspected: Inspected = try call("inspect", ["state": .string(state)])
        guard let cue = inspected.timeline else { throw ScaenaError(message: "\(state) has no cue to inspect") }
        return cue
    }

    /// A digest of `state`'s display list at rest: what changes when its drawing does.
    public func digest(state: String) throws -> String { try call("digest", ["state": .string(state)]) }

    /// `node`'s words as `state` shows them, as written: its `text`, or its runs' end to end. None
    /// for a node that is no text there.
    public func text(state: String, node: String) throws -> String? {
        let carets: JSONValue = try call("carets", ["state": .string(state), "node": .string(node)])
        return carets["text"]?.string
    }

    /// How `state` reads, as the player's live region says it (PLAN 2.8): HTML.
    public func reading(state: String) throws -> String { try call("reading", ["state": .string(state)]) }

    /// Write files back as an undo has them: each to its text, or taken out where it has none.
    public func write(_ files: [(path: String, text: String?)]) throws {
        let listed: [JSONValue] = files.map { file in
            let text: JSONValue = file.text.map { JSONValue.string($0) } ?? .null
            return ["path": .string(file.path), "text": text]
        }
        let _: JSONValue = try call("writeFiles", ["files": .array(listed)])
    }
}

/// What `inspect` says of a state: its cue, the nodes it shows, and those that enter it and leave
/// it, which the cue offers motions for (PLAN 2.44).
struct Inspected: Decodable {
    let timeline: Cue?
    /// The nodes it shows, by id, sorted.
    let nodes: [String]
    let entered: [String]
    let exited: [String]

    private enum Keys: String, CodingKey { case timeline, nodes, entered, exited }

    init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        timeline = try fields.decodeIfPresent(Cue.self, forKey: .timeline)
        nodes = try fields.decodeIfPresent([String: JSONValue].self, forKey: .nodes)?.keys.sorted() ?? []
        entered = try fields.decodeIfPresent([String].self, forKey: .entered) ?? []
        exited = try fields.decodeIfPresent([String].self, forKey: .exited) ?? []
    }
}

extension ScaenaSession.Pixels {
    /// The pixels as an image: straight-alpha sRGB, as the painter gives them.
    public var image: CGImage? {
        guard width > 0, height > 0, rgba.count == width * height * 4,
            let provider = CGDataProvider(data: rgba as CFData),
            let space = CGColorSpace(name: CGColorSpace.sRGB)
        else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4, space: space,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue), provider: provider, decode: nil,
            shouldInterpolate: true, intent: .defaultIntent)
    }
}
