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

/// What an inspector offers (PLAN 2.33, 2.36): each property of a node in a state, or of the
/// state itself, with what it takes, the value shown, and where that value lives.
public struct Choices: Decodable, Sendable {
    /// The node, or none for the state's own.
    public let node: String?
    public let type: String?
    public let state: String
    public let fields: [Field]
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

    public var id: String { prop }

    private enum Keys: String, CodingKey { case prop, takes, value, lives, literal }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        prop = try fields.decode(String.self, forKey: .prop)
        takes = try fields.decode(Takes.self, forKey: .takes)
        value = try fields.decodeIfPresent(JSONValue.self, forKey: .value)
        lives = try fields.decodeIfPresent(Lives.self, forKey: .lives)
        literal = try fields.decodeIfPresent(Bool.self, forKey: .literal) ?? false
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

    /// Its box's corners where it is drawn, clockwise from the top left, canvas units.
    public var corners: [CGPoint] { drawn(rect, through: transform) }
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

        private enum Keys: String, CodingKey { case duration, curve }

        public init(from decoder: any Decoder) throws {
            let fields = try decoder.container(keyedBy: Keys.self)
            duration = try fields.decode(Double.self, forKey: .duration)
            sprung = try fields.decodeIfPresent(JSONValue.self, forKey: .curve)?["spring"] != nil
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
    /// alone, the others' findings kept from the last time they ran on every state.
    public func lint(state: String? = nil) throws -> Linting {
        guard let state else { return try call("lint") }
        return try call("lint", ["state": .string(state)])
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
