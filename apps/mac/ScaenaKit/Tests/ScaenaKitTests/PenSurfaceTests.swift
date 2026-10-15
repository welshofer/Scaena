import CoreGraphics
import Foundation
import Metal
import QuartzCore
import ScaenaKit
import Testing

/// A stroke of the pen shows on the canvas's surface as the slides down the side show it (PLAN
/// 2.114), held as the first criterion of gates 3 and 4 holds a state: drawn across B1's cover
/// through the session, as a straight drag on the canvas draws it, the cover painted again on Metal
/// is within SPEC §13.5 of the CPU painter's, a channel off by under a level on average and none
/// in the drawing's box by half its range or more, as a stroke one painter left out would be. Each
/// draws the stroke in the theme's accent inside the drawing's box, where the surface drew nothing
/// of it before. vello paints it on every GPU but the iPad simulator's, where the CPU painter paints
/// each frame and Metal shows it (PLAN 4.1).
@Test(.enabled(if: MTLCreateSystemDefaultDevice() != nil))
func aStrokeOfThePenPaintsOnMetalAsTheCPUPainterPaintsIt() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "cover"
    let session = editor.session
    // The canvas's own size, a pixel to a unit, as the parity harness holds the painters to §13.5.
    let (width, height) = (1920, 1080)
    let layer = CAMetalLayer()
    layer.drawableSize = CGSize(width: width, height: height)
    let surface = try ScaenaSurface(layer: layer, width: width, height: height)
    try surface.paint(session, state: "cover")
    let blank = try surface.lastFrame()
    // A straight drag across the cover's empty right half, a point every 20 units, as the Mac's walk
    // draws one (`apps/mac/window.sh`).
    let drag = (0...32).map { i in CGPoint(x: 1000 + Double(i) * 20, y: 300 + Double(i) * 2.5) }
    let added = try session.inking(state: "cover", points: drag, onto: nil)
    try editor.make(added.patch)
    let drawn = try #require(try session.boxes(state: "cover").first { $0.node == added.id })
    try surface.paint(session, state: "cover")
    let gpu = try surface.lastFrame()
    let cpu = try session.pixels("cover", width: width)
    try #require(gpu.width == width && gpu.height == height && cpu.width == width && cpu.height == height)
    let bytes = width * height * 4
    try #require(gpu.rgba.count == bytes && cpu.rgba.count == bytes && blank.rgba.count == bytes)
    let (g, c, b) = ([UInt8](gpu.rgba), [UInt8](cpu.rgba), [UInt8](blank.rgba))
    var off = 0
    for i in 0..<bytes { off += abs(Int(g[i]) - Int(c[i])) }
    let mean = Double(off) / Double(bytes)
    #expect(mean < 1.0, "a channel \(mean) of a level off the CPU painter's on average")
    let box = offsets(in: drawn.rect, width: width, height: height)
    #expect(!box.isEmpty, "the drawing's box: \(drawn.rect)")
    var step = 0
    for i in box {
        for k in 0..<4 { step = max(step, abs(Int(g[i + k]) - Int(c[i + k]))) }
    }
    #expect(step < 128, "a channel in the drawing's box \(step) levels off the CPU painter's")
    // The theme's accent (#FF6A3D): red, and far more of it than of blue.
    let accent = { (p: [UInt8], i: Int) -> Bool in Int(p[i]) >= 128 && Int(p[i]) >= Int(p[i + 2]) + 64 }
    let onCPU = box.filter { accent(c, $0) }.count
    let onMetal = box.filter { accent(g, $0) }.count
    let before = box.filter { accent(b, $0) }.count
    #expect(onCPU > 100, "the CPU painter draws the stroke: \(onCPU) pixels in the accent")
    #expect(onMetal > 100, "the surface draws the stroke: \(onMetal) pixels in the accent")
    #expect(before == 0, "the surface drew none of it before it was drawn: \(before) pixels in the accent")
}

/// Where each pixel of a `width` × `height` frame inside `rect`, canvas units a pixel each, starts
/// in its bytes, four to a pixel.
private func offsets(in rect: [Double], width: Int, height: Int) -> [Int] {
    guard rect.count == 4 else { return [] }
    let (x0, y0) = (max(Int(rect[0].rounded(.down)), 0), max(Int(rect[1].rounded(.down)), 0))
    let x1 = min(Int((rect[0] + rect[2]).rounded(.up)), width)
    let y1 = min(Int((rect[1] + rect[3]).rounded(.up)), height)
    guard x0 < x1, y0 < y1 else { return [] }
    return (y0..<y1).flatMap { y in (x0..<x1).map { x in (y * width + x) * 4 } }
}
