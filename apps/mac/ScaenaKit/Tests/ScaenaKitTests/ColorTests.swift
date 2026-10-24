import CoreGraphics
import QuartzCore
import ScaenaKit
import Testing

/// The canvas is color-managed as sRGB (lift-up ATTR-1): its Metal layer says its pixels are sRGB,
/// as the slides' thumbnails, the PNG, and the PDF say theirs are, so on a P3 display a theme's
/// colors show on the canvas as they show everywhere else the deck is drawn.
@Test @MainActor
func theCanvasIsSRGBAsTheThumbnailsAre() throws {
    let view = ScaenaView(frame: CGRect(x: 0, y: 0, width: 320, height: 180))
    let layer = try #require(view.layer as? CAMetalLayer, "the canvas paints on no Metal layer")
    #expect(layer.pixelFormat == .bgra8Unorm, "the painter's eight bits a channel, as they are")
    let space = try #require(layer.colorspace, "the canvas's layer names no color space, so it is not color-matched")
    #expect(space.name == CGColorSpace.sRGB, "the canvas's layer is \(String(describing: space.name)), not sRGB")

    // A thumbnail, drawn by the CPU painter, is in the same space.
    let session = try ScaenaSession(directory: b1)
    let first = try #require(try session.timeline().first)
    let thumbnail = try #require(try session.pixels(first.state, width: 64).image)
    #expect(thumbnail.colorSpace?.name == space.name, "a thumbnail is in another space than the canvas")
}
