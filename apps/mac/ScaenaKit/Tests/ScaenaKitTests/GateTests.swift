import Foundation
import Metal
import QuartzCore
import ScaenaKit
import Testing

/// Gate 3's second criterion (PLAN §Phase 3): a 40-state deck shows its first frame within 300 ms
/// of opening, on an M-series Mac. B1 (SPEC §15) has forty states. Timed as the app opens a deck:
/// from reading the bundle's files, through the session opened and the Metal surface made, to its
/// first state's first frame presented on a layer. The time is printed for the gate log, and where
/// it goes: each stage, and a second surface made once the first is, as a second window's would
/// be. The gate's bar is read on an M-series Mac with the release build (`docs/gate-3.md`). CI's
/// runner links the engine unoptimized (the debug library), runs every test at once, and is a Mac
/// in a virtual machine with a paravirtual GPU: its first reading was 1.39 s, and it is held to
/// five seconds, so a gross regression shows.
@Test(.enabled(if: MTLCreateSystemDefaultDevice() != nil))
func aFortyStateDeckShowsItsFirstFrameSoonAfterOpening() throws {
    let clock = ContinuousClock()
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
    print("gate 3, criterion 2: B1 (\(timeline.count) states), open to first frame: \(took)")
    print(
        "gate 3, criterion 2, where the time goes: files read and session opened \(ms(opened - start)),"
            + " timeline \(ms(timed - opened)), surface \(ms(made - timed)), first frame \(ms(painted - made));"
            + " a second surface \(ms(remade))")
    #expect(timeline.count == 40)
    #expect(took < .seconds(5), "B1's first frame took \(took)")
}

/// `duration` in milliseconds, to a tenth.
private func ms(_ duration: Duration) -> String {
    let (seconds, attoseconds) = duration.components
    return String(format: "%.1f ms", Double(seconds) * 1000 + Double(attoseconds) / 1e15)
}
