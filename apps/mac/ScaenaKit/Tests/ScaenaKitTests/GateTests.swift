import Foundation
import Metal
import QuartzCore
import ScaenaKit
import Testing

/// Same frame, the first criterion of gates 3 and 4: each state of the torture deck, at rest on the
/// deck's own canvas, draws the display list the goldens hold (`tests/golden/torture/raw.fnv1a`),
/// as the browser's engine does (PLAN 0.8). The engine is the one built for where the test runs:
/// the Mac, the iPad simulator, or an iPad, in release there (`apps/ipad/gate.sh`).
@Test func theTortureDeckDrawsTheGoldensDisplayLists() throws {
    let session = try ScaenaSession(directory: tortureDeck)
    var (checked, drawn) = (0, 0)
    for line in try String(contentsOf: goldenDigests, encoding: .utf8).split(separator: "\n") {
        let words = line.split(separator: " ").map(String.init)
        // A frame into a cue (`@`) or in another format (`~`) is the engine's own tests'.
        guard words.count == 2, !words[0].contains("@"), !words[0].contains("~") else { continue }
        let digest = try session.digest(state: words[0])
        #expect(digest == words[1], "\(words[0])")
        checked += 1
        drawn += digest == words[1] ? 1 : 0
    }
    #expect(checked > 40, "\(checked) states checked")
    print(
        "gate \(gate), criterion 1: \(drawn) of the torture deck's \(checked) states at rest draw the goldens'"
            + " display lists")
}

/// What a surface paints on Metal is what the CPU painter paints (the first criterion of gates 3
/// and 4): B1's cover, on a layer no window shows, read back, and how far it is off the CPU
/// painter's printed for the gate. vello paints it on every GPU but the iPad simulator's, which
/// has no indirect dispatch, and where the CPU painter paints each frame and Metal shows it
/// (PLAN 4.1).
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
    let mean = Double(off) / Double(max(cpu.rgba.count, 1))
    let painter = adapter["painter"]?.string ?? "unknown"
    print(
        "gate \(gate), criterion 1: B1's cover painted by \(painter) on Metal, a channel"
            + " \(String(format: "%.3f", mean)) of a level off the CPU painter's on average")
    #expect(mean < 1.0)
    // A surface off by a state paints none, and says why.
    #expect(throws: ScaenaError.self) { try surface.paint(session, state: "no-such-state") }
}

/// The second criterion of gates 3 and 4 (PLAN §Phase 3, §Phase 4): a 40-state deck shows its
/// first frame within 300 ms of opening, on an M-series Mac and on an M-series iPad. B1 (SPEC §15)
/// has forty states. Timed as the app opens a deck: once the GPU is made, as the app makes it when
/// it starts (`ScaenaSurface.warm`), from reading the bundle's files, through the session opened
/// and the Metal surface made, to its first state's first frame presented on a layer. The time is
/// printed for the gate log, with where it goes: the GPU made as the app starts, each stage, and a
/// second surface, as a second window's. Each gate's bar is read with the release build, on an
/// M-series Mac (`docs/gate-3.md`) and on an M-series iPad (`apps/ipad/gate.sh`, `docs/gate-4.md`).
/// CI links the engine unoptimized (the debug library) and runs every test at once, on a Mac in a
/// virtual machine with a paravirtual GPU or on the iPad simulator there: it is held to five
/// seconds, so a gross regression shows.
@Test(.enabled(if: MTLCreateSystemDefaultDevice() != nil))
func aFortyStateDeckShowsItsFirstFrameSoonAfterOpening() throws {
    let clock = ContinuousClock()
    // As the app starts.
    let starting = clock.now
    try ScaenaSurface.warm()
    let started = clock.now - starting
    // As it opens B1.
    let start = clock.now
    let session = try ScaenaSession(directory: b1)
    let opened = clock.now
    let timeline = try session.timeline()
    let first = try #require(timeline.first)
    let timed = clock.now
    let layer = CAMetalLayer()
    layer.drawableSize = CGSize(width: 1920, height: 1080)
    let surface = try ScaenaSurface(layer: layer, width: 1920, height: 1080)
    let made = clock.now
    try surface.paint(session, state: first.state, at: 0)
    let painted = clock.now
    let took = painted - start
    // A surface outlives none of its layer.
    let other = CAMetalLayer()
    let remade = try withExtendedLifetime(other) { () throws -> Duration in
        let again = clock.now
        let second = try ScaenaSurface(layer: other, width: 1920, height: 1080)
        let spent = clock.now - again
        withExtendedLifetime(second) {}
        return spent
    }
    // vello, or the CPU painter on the iPad simulator, whose GPU runs no vello (PLAN 4.1).
    let painter = (try? surface.adapter()["painter"]?.string) ?? "unknown"
    print(
        "gate \(gate), criterion 2: B1 (\(timeline.count) states), open to first frame: \(took),"
            + " painted by \(painter)")
    print(
        "gate \(gate), criterion 2, where the time goes: the GPU made as the app starts \(ms(started)); then"
            + " files read and session opened \(ms(opened - start)), timeline \(ms(timed - opened)),"
            + " surface \(ms(made - timed)), first frame \(ms(painted - made)); a second surface \(ms(remade))."
            + " From the app's start: \(ms(started + took))")
    #expect(timeline.count == 40)
    #expect(took < .seconds(5), "B1's first frame took \(took)")
}

/// `duration` in milliseconds, to a tenth.
private func ms(_ duration: Duration) -> String {
    let (seconds, attoseconds) = duration.components
    return String(format: "%.1f ms", Double(seconds) * 1000 + Double(attoseconds) / 1e15)
}
