import CoreGraphics
import Foundation
import ImageIO
import ScaenaKit
import Testing
import UniformTypeIdentifiers

/// A picture `width` × `height` as `type`: its left half clear, its right half opaque red.
private func picture(_ width: Int, _ height: Int, as type: UTType) throws -> Data {
    let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try #require(
        CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    context.clear(CGRect(x: 0, y: 0, width: width, height: height))
    context.setFillColor(CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1))
    context.fill(CGRect(x: width / 2, y: 0, width: width - width / 2, height: height))
    let image = try #require(context.makeImage())
    let out = NSMutableData()
    let made = try #require(CGImageDestinationCreateWithData(out as CFMutableData, type.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(made, image, nil)
    #expect(CGImageDestinationFinalize(made))
    return out as Data
}

/// The kind of picture `data` is, and its pixel at `x`, `y` (from the top left) as RGBA,
/// premultiplied.
private func read(_ data: Data, at x: Int, _ y: Int) throws -> (kind: String?, pixel: [UInt8]) {
    let source = try #require(CGImageSourceCreateWithData(data as CFData, nil))
    let image = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
    var pixel = [UInt8](repeating: 0, count: 4)
    let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    try pixel.withUnsafeMutableBytes { raw in
        let context = try #require(
            CGContext(
                data: raw.baseAddress, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4, space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        // The picture drawn so that its pixel at x, y lands on the one pixel of the context.
        context.draw(image, in: CGRect(x: -x, y: y - image.height + 1, width: image.width, height: image.height))
    }
    return (CGImageSourceGetType(source) as String?, pixel)
}

/// A picture too large for a slide is read for its size, as the engine reads it, and offered as a
/// smaller copy that fits: a PNG stays a PNG, its transparency kept, and a JPEG a JPEG. The
/// session refuses the picture as it was, leaving nothing behind, and takes the copy.
@Test func aPictureTooLargeGoesInAsASmallerCopy() throws {
    let png = try picture(9000, 40, as: .png)
    let large = try ScaenaSession.picture(png)
    #expect(large == PictureSize(width: 9000, height: 40, largest: 8192))
    #expect(!large.fits)
    #expect(large.fitted.width == 8192 && large.fitted.height == 36)

    let copy = try #require(Pasteboard.smaller(png, longest: large.largest))
    let fits = try ScaenaSession.picture(copy)
    #expect(fits.fits && fits.width == 8192 && fits.height == large.fitted.height)
    let clear = try read(copy, at: 100, 18)
    #expect(clear.kind == UTType.png.identifier)
    #expect(clear.pixel[3] == 0, "its clear half stays clear: \(clear.pixel)")
    let red = try read(copy, at: 8000, 18).pixel
    #expect(red[0] >= 250 && red[1] <= 5 && red[2] <= 5 && red[3] == 255, "its red half stays red: \(red)")

    let jpeg = try picture(9000, 40, as: .jpeg)
    let smaller = try #require(Pasteboard.smaller(jpeg, longest: 8192))
    #expect(try read(smaller, at: 8000, 18).kind == UTType.jpeg.identifier)
    #expect(try ScaenaSession.picture(smaller).width == 8192)

    let session = try ScaenaSession.create(theme: "dusk", title: "Pictures")
    let files = try session.bundleFiles().map(\.path)
    #expect(throws: ScaenaError.self) { try session.drop(png, named: "Wide.png") }
    #expect(try session.bundleFiles().map(\.path) == files, "nothing left behind")
    let path = try session.drop(copy, named: "Wide.png")
    #expect(path.hasPrefix("assets/") && path.hasSuffix(".png"))
}

/// What is no picture an image shows is said to be so, as the engine says it.
@Test func whatIsNoPictureIsSaidToBeSo() {
    #expect(throws: ScaenaError.self) { try ScaenaSession.picture(Data("GIF89a".utf8)) }
}
