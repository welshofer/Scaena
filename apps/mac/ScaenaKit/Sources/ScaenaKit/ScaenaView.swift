import QuartzCore
import SwiftUI

#if os(macOS)
import AppKit

/// The view a canvas paints on: AppKit's on the Mac, UIKit's on the iPad (ADR-0023).
public typealias PlatformView = NSView
#else
import UIKit

/// The view a canvas paints on: AppKit's on the Mac, UIKit's on the iPad (ADR-0023).
public typealias PlatformView = UIView
#endif

/// Where a canvas is in its state's cue: a time into it, ms (infinity: at rest), and whether the
/// cue plays on from there (PLAN 3.4). A place in the deck is a state and a time into its cue,
/// never a place on the global timeline.
public struct Playhead: Equatable, Sendable {
    public var ms: Double
    public var playing: Bool

    public init(ms: Double = 0, playing: Bool = true) {
        self.ms = ms
        self.playing = playing
    }

    /// At rest, the cue over.
    public static let rest = Playhead(ms: .infinity, playing: false)
}

/// A state of a deck, painted by the engine on Metal and presented at the display's refresh,
/// up to 120 Hz on ProMotion (PLAN 3.2, SPEC §9.3). It shows its playhead (PLAN 3.4): a time
/// into the state's cue, held, or the cue played on from it by the view's own display link,
/// then at rest. Once nothing moves, it idles until the state, the playhead, the deck, or the
/// size changes. SwiftUI shows it through `ScaenaCanvas`. On the iPad it is a `UIView` on the
/// same layer, painted the same way (PLAN 4.1, SPEC §9.4).
@MainActor
public final class ScaenaView: PlatformView {
    /// The session whose frames it shows.
    public var session: ScaenaSession? {
        didSet { restart() }
    }

    /// The state it shows. A new one plays from the playhead, as the window sets it.
    public var state: String? {
        didSet { if state != oldValue { restart() } }
    }

    /// Changed by each edit of the deck: the state is painted again where the playhead is.
    public var revision = 0 {
        didSet {
            guard revision != oldValue else { return }
            cue = (session.flatMap { s in state.flatMap { try? s.duration(of: $0) } }) ?? cue
            dirty = true
        }
    }

    /// Where it is in the cue: what `show` set, and while the cue plays, the time it painted last.
    public private(set) var playhead = Playhead()

    /// Told where the playhead is while the cue plays, some 30 times a second, and when it comes
    /// to rest.
    public var onPlayhead: (@MainActor (Playhead) -> Void)?

    /// Why the last frame could not be painted, if it could not.
    public private(set) var failure: ScaenaError?

    private var surface: ScaenaSurface?
    private var link: CADisplayLink?
    /// When the cue would have begun, on the display link's clock, for the time it plays from.
    private var began: CFTimeInterval?
    /// When the playhead was last told, on the display link's clock.
    private var told: CFTimeInterval = 0
    /// How long the cue runs, ms: past it, the state is at rest.
    private var cue: Double = 0
    /// Whether the frame shown is not the playhead's.
    private var dirty = true

    #if os(macOS)
    public override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
    }

    public required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
    }

    public override func makeBackingLayer() -> CALayer {
        let layer = CAMetalLayer()
        layer.pixelFormat = .bgra8Unorm
        return layer
    }

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        attach(window == nil ? nil : displayLink(target: self, selector: #selector(step(_:))))
    }

    public override func setFrameSize(_ size: NSSize) {
        super.setFrameSize(size)
        resized()
    }

    public override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        resized()
    }

    /// Device pixels to a point: the window's backing scale.
    private var scale: CGFloat { window?.backingScaleFactor ?? metalLayer?.contentsScale ?? 1 }
    #else
    public override class var layerClass: AnyClass { CAMetalLayer.self }

    public override init(frame: CGRect) {
        super.init(frame: frame)
        made()
    }

    public required init?(coder: NSCoder) {
        super.init(coder: coder)
        made()
    }

    private func made() {
        metalLayer?.pixelFormat = .bgra8Unorm
        registerForTraitChanges([UITraitDisplayScale.self]) { (view: ScaenaView, _: UITraitCollection) in
            view.resized()
        }
    }

    public override func didMoveToWindow() {
        super.didMoveToWindow()
        attach(window == nil ? nil : CADisplayLink(target: self, selector: #selector(step(_:))))
    }

    /// Laid out again, as UIKit does whenever anything about the view may have changed: painted
    /// again only where its size in pixels did.
    public override func layoutSubviews() {
        super.layoutSubviews()
        if metalLayer?.drawableSize != pixels { resized() }
    }

    /// Device pixels to a point: the screen's scale, as the view's traits give it.
    private var scale: CGFloat { max(traitCollection.displayScale, 1) }
    #endif

    private var metalLayer: CAMetalLayer? { layer as? CAMetalLayer }

    /// Paint on `next`'s ticks, asking for 120 Hz, in place of the link it had; none while the
    /// view is in no window.
    private func attach(_ next: CADisplayLink?) {
        link?.invalidate()
        link = next
        guard let next else { return }
        next.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 120)
        next.add(to: .main, forMode: .common)
        resized()
    }

    /// Show `next`: a time into the cue, held, or the cue played on from it. A playhead the view
    /// told itself changes nothing.
    public func show(_ next: Playhead) {
        guard next != playhead else { return }
        // A cue that plays keeps its own clock; one that starts, or holds, begins again.
        if !(next.playing && playhead.playing) { began = nil }
        playhead = next
        dirty = true
    }

    /// The view's size in device pixels, at least one each way.
    private var pixels: CGSize { CGSize(width: max(bounds.width * scale, 1), height: max(bounds.height * scale, 1)) }

    private func resized() {
        guard let layer = metalLayer else { return }
        let size = pixels
        layer.contentsScale = scale
        layer.drawableSize = size
        surface?.resize(width: Int(size.width), height: Int(size.height))
        dirty = true
    }

    private func restart() {
        began = nil
        dirty = true
        if let session, let state {
            cue = (try? session.duration(of: state)) ?? 0
        }
    }

    @objc private func step(_ link: CADisplayLink) {
        guard dirty || playhead.playing, let session, let state, let layer = metalLayer, bounds.width > 0
        else { return }
        do {
            if surface == nil {
                let size = layer.drawableSize
                surface = try ScaenaSurface(layer: layer, width: Int(size.width), height: Int(size.height))
            }
            var at = playhead.ms
            var over = false
            if playhead.playing {
                let from = playhead.ms.isFinite && playhead.ms < cue ? max(playhead.ms, 0) : 0
                let start = began ?? link.timestamp - from / 1000
                began = start
                let ms = (link.timestamp - start) * 1000
                over = ms >= cue
                at = over ? .infinity : ms
            }
            // A layer with no drawable to give skips the frame: the next tick paints it.
            guard try surface?.paint(session, state: state, at: at) == true else { return }
            failure = nil
            dirty = false
            guard playhead.playing else { return }
            if over {
                playhead = .rest
                began = nil
                tell(link.timestamp)
            } else {
                playhead.ms = at
                if link.timestamp - told >= 1.0 / 30 { tell(link.timestamp) }
            }
        } catch {
            failure = error as? ScaenaError ?? ScaenaError(message: "\(error)")
            dirty = false
            if playhead.playing {
                playhead = .rest
                began = nil
                tell(link.timestamp)
            }
        }
    }

    private func tell(_ now: CFTimeInterval) {
        told = now
        onPlayhead?(playhead)
    }
}

#if os(macOS)
/// What SwiftUI shows a platform's view through: AppKit's on the Mac, UIKit's on the iPad.
public typealias PlatformViewRepresentable = NSViewRepresentable
#else
/// What SwiftUI shows a platform's view through: AppKit's on the Mac, UIKit's on the iPad.
public typealias PlatformViewRepresentable = UIViewRepresentable
#endif

/// `ScaenaView` in SwiftUI: `state` of `session` where `playhead` is, painted again when
/// `revision` changes, after an edit. While the cue plays, the view moves the playhead.
public struct ScaenaCanvas: PlatformViewRepresentable {
    public let session: ScaenaSession
    public let state: String
    public let revision: Int
    @Binding public var playhead: Playhead

    public init(session: ScaenaSession, state: String, revision: Int = 0, playhead: Binding<Playhead>) {
        self.session = session
        self.state = state
        self.revision = revision
        self._playhead = playhead
    }

    #if os(macOS)
    public func makeNSView(context: Context) -> ScaenaView {
        let view = ScaenaView(frame: .zero)
        update(view)
        return view
    }

    public func updateNSView(_ view: ScaenaView, context: Context) {
        update(view)
    }
    #else
    public func makeUIView(context: Context) -> ScaenaView {
        let view = ScaenaView(frame: .zero)
        update(view)
        return view
    }

    public func updateUIView(_ view: ScaenaView, context: Context) {
        update(view)
    }
    #endif

    private func update(_ view: ScaenaView) {
        if view.session !== session { view.session = session }
        if view.state != state { view.state = state }
        view.revision = revision
        let playhead = $playhead
        view.onPlayhead = { playhead.wrappedValue = $0 }
        view.show(self.playhead)
    }
}
