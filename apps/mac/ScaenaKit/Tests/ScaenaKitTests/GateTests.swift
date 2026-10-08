import Foundation
import Metal
import QuartzCore
import ScaenaKit
import Testing

/// Gate 3's second criterion (PLAN §Phase 3): a 40-state deck shows its first frame within 300 ms
/// of opening, on an M-series Mac. B1 (SPEC §15) has forty states. Timed as the app opens a deck:
/// from reading the bundle's files, through the session opened and the Metal surface made, to its
/// first state's first frame presented on a layer. The time is printed for the gate log. A real
/// Mac reads the gate's bar; CI's runner, a Mac in a virtual machine with a paravirtual GPU, is
/// held to a second, so a regression shows.
@Test(.enabled(if: MTLCreateSystemDefaultDevice() != nil))
func aFortyStateDeckShowsItsFirstFrameSoonAfterOpening() throws {
    let clock = ContinuousClock()
    let start = clock.now
    let session = try ScaenaSession(directory: b1)
    let timeline = try session.timeline()
    let first = try #require(timeline.first)
    let layer = CAMetalLayer()
    layer.drawableSize = CGSize(width: 1920, height: 1080)
    let surface = try ScaenaSurface(layer: layer, width: 1920, height: 1080)
    try surface.paint(session, state: first.state, at: 0)
    let took = clock.now - start
    print("gate 3, criterion 2: B1 (\(timeline.count) states), open to first frame: \(took)")
    #expect(timeline.count == 40)
    #expect(took < .seconds(1), "B1's first frame took \(took)")
}
