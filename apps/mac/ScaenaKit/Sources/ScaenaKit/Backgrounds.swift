import CScaena
import CoreGraphics
import Foundation

/// One background a slide may take (PLAN 2.148, 3.32): None, or one of the theme's shader presets,
/// by its name and its kind in a person's words, with the patch that gives the slide it, none for
/// the one it has.
public struct BackgroundOption: Decodable, Sendable, Identifiable, Equatable {
    /// The theme's preset; nil for None.
    public let preset: String?
    /// Its kind of shader (`mesh`, `noise`, …); nil for None.
    public let kind: String?
    /// Its name in words: Backdrop, or None.
    public let label: String
    /// Its kind in words: Mesh Gradient.
    public let said: String?
    /// The slide's background now.
    public let current: Bool
    public let patch: [JSONValue]
    /// Its picture's size, pixels.
    public let width: Int
    public let height: Int

    public var id: String { preset ?? "" }

    private enum CodingKeys: String, CodingKey {
        case preset, kind, label, said, current, patch, width, height
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        preset = try c.decodeIfPresent(String.self, forKey: .preset)
        kind = try c.decodeIfPresent(String.self, forKey: .kind)
        label = try c.decode(String.self, forKey: .label)
        said = try c.decodeIfPresent(String.self, forKey: .said)
        current = try c.decodeIfPresent(Bool.self, forKey: .current) ?? false
        patch = try c.decode([JSONValue].self, forKey: .patch)
        width = try c.decode(Int.self, forKey: .width)
        height = try c.decode(Int.self, forKey: .height)
    }
}

/// What a slide may take as its background (PLAN 2.148, 3.32): its first state, where one enters;
/// why backgrounds stay off it where it shows a chart or a table (lint W311), each then offered
/// dimmed; and None, then each of the theme's presets.
public struct Backgrounds: Decodable, Sendable, Equatable {
    public let first: String
    public let held: String?
    public let options: [BackgroundOption]

    /// Whether `option` may be chosen: None always, a preset where the slide shows no data.
    public func offers(_ option: BackgroundOption) -> Bool {
        held == nil || option.preset == nil
    }
}

extension ScaenaSession {
    /// What the slide of `state` may take as its background, each drawn as the slide shows it with
    /// it, at rest, `height` pixels high in the format shown; `backgroundPicture` paints each.
    public func backgrounds(state: String, height: Int) throws -> Backgrounds {
        try call("backgroundsPainted", ["state": .string(state), "height": .number(Double(max(height, 1)))])
    }

    /// The `i`th background `backgrounds` drew last, its effect's clock at `t` seconds (SPEC §3.8).
    public func backgroundPicture(_ i: Int, at t: Double = 0) throws -> Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let painted = scaena_background_pixels(handle, i, t, &error)
        let rgba = try Self.take(painted.bytes, error)
        return Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height))
    }

    /// What the slide of `state` may take as its background, each with its picture `height`
    /// pixels high, still: one whose picture cannot be made has none.
    public func backgroundsDrawn(state: String, height: Int) throws -> (Backgrounds, [CGImage?]) {
        let offered = try backgrounds(state: state, height: height)
        let images = offered.options.indices.map { i in (try? backgroundPicture(i))?.image }
        return (offered, images)
    }
}
