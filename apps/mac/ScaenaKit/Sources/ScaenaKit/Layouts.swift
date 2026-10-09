import CScaena
import CoreGraphics
import Foundation

/// A layout a state may take (PLAN 2.92, 3.26), judged as `scaena inspect --layouts` judges it:
/// what lint finds in the state laid out in it, in every format the deck lists; the layouts that
/// draw it alike, folded into this one; the states its patch changes; and the patch, one
/// `set_state`, none for the layout the state takes now.
public struct LayoutSuggestion: Decodable, Sendable, Identifiable, Equatable {
    public let layout: String
    /// The layout the state takes now.
    public let current: Bool
    public let errors: Int
    public let warnings: Int
    public let alike: [String]
    public let reach: [String]
    public let patch: [JSONValue]
    /// Its picture's size, pixels.
    public let width: Int
    public let height: Int

    public var id: String { layout }

    private enum CodingKeys: String, CodingKey {
        case layout, current, errors, warnings, alike, reach, patch, width, height
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        layout = try c.decode(String.self, forKey: .layout)
        current = try c.decodeIfPresent(Bool.self, forKey: .current) ?? false
        errors = try c.decode(Int.self, forKey: .errors)
        warnings = try c.decode(Int.self, forKey: .warnings)
        alike = try c.decodeIfPresent([String].self, forKey: .alike) ?? []
        reach = try c.decode([String].self, forKey: .reach)
        patch = try c.decode([JSONValue].self, forKey: .patch)
        width = try c.decode(Int.self, forKey: .width)
        height = try c.decode(Int.self, forKey: .height)
    }
}

extension ScaenaSession {
    /// Begin judging the layouts `state` may take (PLAN 2.92, 3.26): how many there are. Each is
    /// judged by a `layoutsStep`, so that the window answers a person between them.
    public func layoutsBegin(state: String) throws -> Int {
        try call("layoutsBegin", ["state": .string(state)])
    }

    /// Judge the next layout: the state laid out in it, linted in every format, and drawn in the
    /// format shown. Whether any is left. An edit since `layoutsBegin` ends the round, an error.
    public func layoutsStep() throws -> Bool {
        try call("layoutsStep")
    }

    /// The layouts judged since `layoutsBegin`, best first, each painted at rest `height` pixels
    /// high; the round ends. `layoutPicture(i)` takes the `i`th one's picture.
    public func layoutSuggestions(height: Int) throws -> [LayoutSuggestion] {
        try call("layoutSuggestions", ["height": .number(Double(max(height, 1)))])
    }

    /// The `i`th picture `layoutSuggestions` painted last, taken: a second call throws.
    public func layoutPicture(_ i: Int) throws -> Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let painted = scaena_layout_pixels(handle, i, &error)
        let rgba = try Self.take(painted.bytes, error)
        return Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height))
    }

    /// The layouts judged since `layoutsBegin`, best first, each with its picture `height` pixels
    /// high; the round ends. One whose picture cannot be made is left out.
    public func layoutsDrawn(height: Int) throws -> [(suggestion: LayoutSuggestion, image: CGImage)] {
        try layoutSuggestions(height: height).enumerated().compactMap { i, suggestion in
            (try layoutPicture(i)).image.map { (suggestion: suggestion, image: $0) }
        }
    }
}
