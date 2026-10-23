#if os(macOS)
import AppKit
#else
import UIKit
#endif
import CoreGraphics
import Foundation
import Metal
import ScaenaKit
import Testing

/// The canvas's display link rests while nothing moves (lift-up PERF-2): a frame owed, the frame
/// shown not the playhead's or the cue playing on, it ticks; the frame painted and the cue at rest,
/// a tick finds nothing to paint and rests the link, so an open deck wakes the main thread for no
/// frame it does not paint; an edit, a resize, a playhead shown, or a cue played wakes it. The view
/// is in a window of its own, where its link is made, and ticked by hand, as the link ticks it.
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
    let view = ScaenaView(frame: CGRect(x: 0, y: 0, width: 320, height: 180))
    #expect(!view.ticking, "in no window, there is no link to tick")
    #if os(macOS)
    let window = NSWindow(
        contentRect: CGRect(x: 0, y: 0, width: 320, height: 180), styleMask: [.borderless], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = view
    // On screen: a layer the window server does not show gives no drawable.
    window.orderFrontRegardless()
    #else
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 180))
    window.addSubview(view)
    window.isHidden = false
    #endif
    defer {
        #if os(macOS)
        window.orderOut(nil)
        window.contentView = nil
        #else
        window.isHidden = true
        view.removeFromSuperview()
        #endif
    }
    #expect(view.ticking, "in a window, a frame is owed, and the link ticks")

    // Nothing to paint yet: a tick rests the link; the session and the state wake it.
    view.tick(at: 0)
    #expect(!view.ticking, "with no deck, a tick rests the link")
    view.session = session
    view.state = first.state
    view.show(.rest)
    #expect(view.ticking, "a deck to paint wakes it")
    #expect(paints(view, at: 1 / 120), "the frame paints: \(view.failure?.message ?? "no drawable")")
    #expect(view.ticking, "the frame painted, the link ticks once more")
    view.tick(at: 2 / 120)
    #expect(!view.ticking, "which finds nothing to paint, and rests it")

    // An edit wakes it, for one frame.
    view.revision += 1
    #expect(view.ticking, "an edit wakes it")
    #expect(paints(view, at: 3 / 120), "the edit paints")
    view.tick(at: 4 / 120)
    #expect(!view.ticking, "the edit painted, it rests")

    // The cue played wakes it for as long as the cue runs, then it rests again.
    view.state = played.state
    view.show(Playhead(ms: 0, playing: true))
    #expect(view.ticking, "a cue played wakes it")
    let began: CFTimeInterval = 10
    #expect(paints(view, at: began), "the cue's first frame paints")
    #expect(paints(view, at: began + played.span / 2000), "a frame inside the cue paints")
    #expect(view.ticking && view.playhead.playing, "it ticks while the cue plays: \(view.playhead)")
    #expect(paints(view, at: began + played.span / 1000 + 0.1), "the frame at rest paints")
    #expect(view.playhead == .rest, "the cue over, the state is at rest")
    view.tick(at: began + played.span / 1000 + 0.2)
    #expect(!view.ticking, "and the link rests")
    // A playhead shown wakes it: the frame held.
    view.show(Playhead(ms: played.span / 2, playing: false))
    #expect(view.ticking, "a playhead shown wakes it")
    #expect(paints(view, at: 20), "the frame held paints")
    view.tick(at: 20 + 1 / 120)
    #expect(!view.ticking, "the frame held, it rests")
}
