import CoreGraphics
import Foundation
import Metal
import ScaenaKit
import Testing

/// The canvas's display link rests while nothing moves (lift-up PERF-2): a frame owed, the frame
/// shown not the playhead's or the cue playing on, the view paints and its link ticks; the frame
/// painted and the cue at rest, a tick finds nothing to paint and the link rests, so an open deck
/// wakes the main thread for no frame it does not paint; an edit, a resize, a playhead shown, or a
/// cue played wakes it. The view is in no window, where its Metal layer gives drawables as the gate
/// tests' do (one in a window of a test process does not), and is ticked by hand, as its link
/// ticks it.
@Test(.enabled(if: MTLCreateSystemDefaultDevice() != nil)) @MainActor
func theDisplayLinkRestsWhileNothingMoves() throws {
    /// `view` ticked at `now` until a frame paints: its layer may have no drawable to give for a
    /// tick or two, which the view leaves to the next tick, as the link's would.
    func paints(_ view: ScaenaView, at now: CFTimeInterval) -> Bool {
        for _ in 0..<120 {
            if view.tick(at: now) { return true }
            if view.failure != nil { return false }
            Thread.sleep(forTimeInterval: 1.0 / 60)
        }
        return false
    }
    let session = try ScaenaSession(directory: b1)
    let slots = try session.timeline()
    let first = try #require(slots.first)
    // A state whose cue runs, to play.
    let played = try #require(slots.first { $0.span > 0 })
    let view = ScaenaView(frame: .zero)
    // Sized as a window would size it: its layer takes the size in pixels.
    view.frame = CGRect(x: 0, y: 0, width: 320, height: 180)
    #if !os(macOS)
    view.layoutIfNeeded()
    #endif
    #expect(view.painting, "a frame is owed as it is made")

    // Nothing to paint yet: a tick finds no deck, and the view idles until the session and the
    // state are set.
    #expect(!view.tick(at: 0), "with no deck, nothing paints")
    #expect(view.painting, "a frame is still owed")
    view.session = session
    view.state = first.state
    view.show(.rest)
    #expect(view.painting, "a deck to paint")
    #expect(paints(view, at: 1 / 120), "the frame paints: \(view.failure?.message ?? "no drawable")")
    #expect(!view.painting, "the frame painted, nothing is owed, and the link rests")
    #expect(!view.tick(at: 2 / 120), "a tick finds nothing to paint")

    // An edit wakes it, for one frame.
    view.revision += 1
    #expect(view.painting, "an edit wakes it")
    #expect(paints(view, at: 3 / 120), "the edit paints")
    #expect(!view.painting, "the edit painted, it rests")

    // The cue played wakes it for as long as the cue runs, then it rests again.
    view.state = played.state
    view.show(Playhead(ms: 0, playing: true))
    #expect(view.painting, "a cue played wakes it")
    let began: CFTimeInterval = 10
    #expect(paints(view, at: began), "the cue's first frame paints")
    #expect(paints(view, at: began + played.span / 2000), "a frame inside the cue paints")
    #expect(view.painting && view.playhead.playing, "it paints on while the cue plays: \(view.playhead)")
    #expect(paints(view, at: began + played.span / 1000 + 0.1), "the frame at rest paints")
    #expect(view.playhead == .rest, "the cue over, the state is at rest")
    #expect(!view.painting, "and it rests")
    // A playhead shown wakes it: the frame held.
    view.show(Playhead(ms: played.span / 2, playing: false))
    #expect(view.painting, "a playhead shown wakes it")
    #expect(paints(view, at: 20), "the frame held paints")
    #expect(!view.painting, "the frame held, it rests")
}
