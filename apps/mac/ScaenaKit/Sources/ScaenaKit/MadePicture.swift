import CoreGraphics
import Foundation

/// A picture made (PLAN 2.107): its bytes, its file's kind (`png`, `jpg`), what it shows, and the
/// model that made it.
public struct MadePicture: Sendable, Equatable {
    public let bytes: Data
    public let ext: String
    public let alt: String
    public let model: String

    public init(bytes: Data, ext: String, alt: String, model: String) {
        self.bytes = bytes
        self.ext = ext
        self.alt = alt
        self.model = model
    }

    /// Its file's name: the first few words of what it shows, in lowercase, joined by hyphens, as
    /// the browser names one; the name names its image.
    public var name: String {
        let words = alt.lowercased().folding(options: .diacriticInsensitive, locale: nil)
            .split { !($0.isASCII && ($0.isLetter || $0.isNumber)) }
            .prefix(5)
        return "\(words.isEmpty ? "picture" : words.joined(separator: "-")).\(ext)"
    }

    /// What it shows, mid-sentence: "A lighthouse" as "a lighthouse", "NASA" as it is.
    public var shows: String {
        let said = alt.trimmingCharacters(in: CharacterSet(charactersIn: ".!"))
        let word = said.prefix { $0.isLetter }
        guard let first = word.first, first.isUppercase, word.dropFirst().allSatisfy(\.isLowercase) else { return said }
        return first.lowercased() + said.dropFirst()
    }
}

/// Where a picture made for the slide shown goes (PLAN 2.107): the image `node`, which shows it in
/// place of what it showed (`shows`, what that image says it shows); the empty place for a picture
/// `slot`; or neither, inserted about `point`. Its place is `size`, canvas units, which frames the
/// picture wide, tall, or square.
public struct PictureTarget: Sendable, Equatable, Identifiable {
    public let node: String?
    public let slot: String?
    public let point: CGPoint
    public let size: CGSize
    public let shows: String?

    /// What it is to a sheet presented for it: the image, the place, or the point.
    public var id: String { node ?? slot ?? "\(point.x),\(point.y)" }

    /// How its place frames a picture, as the generator frames it: `wide`, `tall`, or `square`.
    public var frame: String {
        let ratio = size.height > 0 ? size.width / size.height : 1
        return ratio >= 1.2 ? "wide" : ratio <= 0.84 ? "tall" : "square"
    }
}

/// A picture put where it was asked for: the patch, one step to undo, the node it selects, and
/// what the window says of it.
public struct PicturePut: Sendable {
    public let patch: [JSONValue]
    public let node: String
    public let said: String
}

extension ScaenaSession {
    /// Where a picture made for `state` goes (PLAN 2.107), as the browser finds it: the image
    /// `selected`, alone, which shows it in place of what it showed; else the first of the slide's
    /// empty places for a picture; else about `pressed`, where the pointer last pressed, or the
    /// middle, in the canvas's own shape.
    public func pictureTarget(state: String, selected: [String], pressed: CGPoint?) throws -> PictureTarget {
        if selected.count == 1, let node = selected.first,
            let box = try boxes(state: state).first(where: { $0.node == node }), box.rect.count == 4,
            let offered = try? self.choices(state: state, node: node), offered.type == "image"
        {
            let r = box.rect
            let shows = offered.fields.first { $0.prop == "alt" }?.value?.string
            return PictureTarget(
                node: node, slot: nil, point: CGPoint(x: r[0] + r[2] / 2, y: r[1] + r[3] / 2),
                size: CGSize(width: r[2], height: r[3]), shows: shows?.isEmpty == false ? shows : nil)
        }
        if let place = try waiting(state: state).first(where: \.picture) {
            return PictureTarget(
                node: nil, slot: place.slot, point: CGPoint(x: place.rect.midX, y: place.rect.midY), size: place.rect.size,
                shows: nil)
        }
        let size = try canvasSize()
        return PictureTarget(
            node: nil, slot: nil, point: pressed ?? CGPoint(x: size.width / 2, y: size.height / 2), size: size, shows: nil)
    }

    /// The picture `made` put where `target` says in `state` (PLAN 2.107): it joins the bundle,
    /// named by its content, as one dropped does. The image it was made for shows it, saying what
    /// it shows, one `choose` of `src` and one of `alt`; else it is inserted as Insert inserts a
    /// picture, filling the empty place it was made for, saying what it shows.
    public func puttingPicture(_ made: MadePicture, at target: PictureTarget, in state: String) throws -> PicturePut {
        let path = try drop(made.bytes, named: made.name)
        let by = "made by \(made.model)"
        if let node = target.node {
            let patch: [JSONValue] = [("src", path), ("alt", made.alt)].map { prop, value in
                ["op": "choose", "node": .string(node), "prop": .string(prop), "value": .string(value), "state": .string(state)]
            }
            return PicturePut(patch: patch, node: node, said: "Picture replaced with one of \(made.shows), \(by)")
        }
        let offered = try inserts()
        guard let n = offered.firstIndex(where: { $0.node["type"]?.string == "image" && $0.node["src"]?.string == path })
        else {
            throw ScaenaError(message: "the picture made is not one the deck can show")
        }
        let added = try inserting(state: state, n: n, at: target.point, named: made.name, with: ["alt": .string(made.alt)])
        return PicturePut(patch: added.patch, node: added.id, said: "Picture of \(made.shows) added, \(by)")
    }
}
