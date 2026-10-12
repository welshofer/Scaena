import CScaena
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// A picture as the engine reads it before it joins the bundle (SPEC §3.3): its size as it is
/// seen, a JPEG's turned by its EXIF orientation, and the longest side an image may have. One too
/// large is offered as a smaller copy, never refused (`Pasteboard.smaller`).
public struct PictureSize: Decodable, Equatable, Sendable {
    public let width: Int
    public let height: Int
    /// The longest side an image may have, in pixels.
    public let largest: Int

    public init(width: Int, height: Int, largest: Int) {
        self.width = width
        self.height = height
        self.largest = largest
    }

    /// Whether an image shows it as it is.
    public var fits: Bool { max(width, height) <= largest }

    /// Its size made to fit: its longer side the longest an image may have, the other in
    /// proportion, as `Pasteboard.smaller` makes it.
    public var fitted: (width: Int, height: Int) {
        guard !fits else { return (width, height) }
        let scale = Double(largest) / Double(max(width, height))
        return (
            max(1, Int((Double(width) * scale).rounded())), max(1, Int((Double(height) * scale).rounded()))
        )
    }
}

extension ScaenaSession {
    /// What `data`, a PNG or a JPEG, is as a picture, read as the engine reads it; thrown where
    /// it is no picture an image shows.
    public static func picture(_ data: Data) throws -> PictureSize {
        let answer = data.withUnsafeBytes { raw in
            scaena_picture(raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count)
        }
        return try decode(answer, as: PictureSize.self)
    }
}

extension Pasteboard {
    /// A copy of `data`, a picture too large for a slide, made smaller: its longer side
    /// `longest` pixels, the other in proportion, upright as it is seen. A PNG stays a PNG, its
    /// transparency kept; a JPEG stays a JPEG. The picture it was made from is left as it was.
    /// None where ImageIO cannot read it.
    public static func smaller(_ data: Data, longest: Int) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let kind = CGImageSourceGetType(source)
        else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            // Turned by its EXIF orientation, as the engine shows it, and written upright.
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: longest,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        let png = UTType(kind as String)?.conforms(to: .png) == true
        let made = NSMutableData()
        let type = (png ? UTType.png : UTType.jpeg).identifier as CFString
        guard let out = CGImageDestinationCreateWithData(made as CFMutableData, type, 1, nil) else { return nil }
        let properties: [CFString: Any] = png ? [:] : [kCGImageDestinationLossyCompressionQuality: 0.92]
        CGImageDestinationAddImage(out, image, properties as CFDictionary)
        return CGImageDestinationFinalize(out) ? made as Data : nil
    }
}
