import Foundation
import Metal
import QuartzCore
import ScaenaKit
import Testing

/// The repository's root, from this file's place in it.
let repository = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()  // ScaenaKitTests
    .deletingLastPathComponent()  // Tests
    .deletingLastPathComponent()  // ScaenaKit
    .deletingLastPathComponent()  // mac
    .deletingLastPathComponent()  // apps
    .deletingLastPathComponent()

/// B1 (SPEC §15): forty states over four families of type.
let b1 = repository.appending(path: "tests/bench/b1.scaena")

@Test func aBundleOpensFromItsFolderAndDraws() throws {
    let session = try ScaenaSession(directory: b1)
    let states = try session.states()
    #expect(states.count == 40)
    #expect(states.first == "cover")
    let timeline = try session.timeline()
    #expect(timeline.map(\.state) == states)
    #expect(timeline[0].slide == "cover")
    #expect(try !session.frame("cover").isEmpty)
    let pixels = try session.pixels("cover", width: 160)
    #expect(pixels.width == 160 && pixels.height == 90)
    #expect(pixels.rgba.count == 160 * 90 * 4)
}

@Test func anEditIsThePatchTheBrowserMakes() throws {
    let session = try ScaenaSession(directory: b1)
    let source = try session.source()
    let compiled = try session.compile(source)
    #expect(compiled.valid && compiled.error == nil)
    #expect(compiled.states.first?.state == "cover")
    let linted = try session.lint()
    #expect(linted.whole && linted.laid)
    let state = try session.states()[0]
    let inspected: JSONValue = try session.call("inspect", ["state": .string(state)])
    let node = try #require(inspected["looks"]?.object?.keys.sorted().first)
    let op: JSONValue = [
        "op": "replace_text", "state": .string(state), "node": .string(node), "from": 0, "to": 0, "text": "Set in Swift. ",
    ]
    let called = try session.tool("deck_patch", ["ops": [op]])
    #expect(called.edited)
    #expect(try session.source().contains("Set in Swift."))
}

@Test func aSaveIsWrittenZippedReopenedAndAdopted() throws {
    let session = try ScaenaSession(directory: b1)
    let saved = try session.save(subset: true)
    #expect(saved.files.contains("deck.json"))
    #expect(saved.file("deck.json") != nil)
    #expect(saved.file("no/such/file") == nil)
    let reopened = try ScaenaSession(zip: try saved.zip())
    #expect(try reopened.states() == session.states())
    try session.adopt(saved)
}

@Test func whatTheEngineCannotDoIsAnErrorThatSaysWhy() throws {
    let session = try ScaenaSession(directory: b1)
    #expect(throws: ScaenaError.self) { try session.call("nonsense") as JSONValue }
    #expect(throws: ScaenaError.self) { try session.frame("no-such-state") }
    #expect(throws: ScaenaError.self) { try ScaenaSession(files: [:]) }
    #expect(throws: ScaenaError.self) { try ScaenaSession(zip: Data("not a zip".utf8)) }
}

/// What a surface paints on Metal is what the CPU painter paints (gate 3's first criterion):
/// B1's cover, on a layer no window shows, read back. vello paints it on every GPU but the iPad
/// simulator's, which has no indirect dispatch, and where the CPU painter paints each frame and
/// Metal shows it (PLAN 4.1).
@Test(.enabled(if: MTLCreateSystemDefaultDevice() != nil))
func aSurfacePaintsOnMetalWhatTheCPUPainterPaints() throws {
    let session = try ScaenaSession(directory: b1)
    let layer = CAMetalLayer()
    layer.drawableSize = CGSize(width: 320, height: 180)
    let surface = try ScaenaSurface(layer: layer, width: 320, height: 180)
    let adapter = try surface.adapter()
    #expect(adapter["backend"]?.string == "Metal")
    #if targetEnvironment(simulator)
    #expect(adapter["painter"]?.string == "cpu")
    #else
    #expect(adapter["painter"]?.string == "vello")
    #endif
    try surface.paint(session, state: "cover")
    let gpu = try surface.lastFrame()
    let cpu = try session.pixels("cover", width: 320)
    #expect(gpu.width == cpu.width && gpu.height == cpu.height)
    #expect(gpu.rgba.count == cpu.rgba.count)
    // The painters differ at anti-aliased edges alone (SPEC §13.5): a channel off by a level or
    // two there, nothing more on average.
    var off = 0
    for (a, b) in zip(gpu.rgba, cpu.rgba) { off += abs(Int(a) - Int(b)) }
    #expect(Double(off) / Double(max(cpu.rgba.count, 1)) < 1.0)
    // A surface off by a state paints none, and says why.
    #expect(throws: ScaenaError.self) { try surface.paint(session, state: "no-such-state") }
}
