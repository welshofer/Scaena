import AppKit
import QuartzCore
import SwiftUI

/// A state of a deck, painted by the engine on Metal and presented at the display's refresh,
/// up to 120 Hz on ProMotion (PLAN 3.2, SPEC §9.3). Setting `state` plays its cue from the
/// start; once the cue is over the state is at rest, painted once more, and the view idles until
/// something changes. SwiftUI shows it through `ScaenaCanvas`.
@MainActor
public final class ScaenaView: NSView {
    /// The session whose frames it shows.
    public var session: ScaenaSession? {
        didSet { restart() }
    }

    /// The state it shows.
    public var state: String? {
        didSet { restart() }
    }

    /// Why the last frame could not be painted, if it could not.
    public private(set) var failure: ScaenaError?

    private var surface: ScaenaSurface?
    private var link: CADisplayLink?
    /// When the cue began, on the display link's clock.
    private var began: CFTimeInterval?
    /// How long the cue runs, ms: past it, the state is at rest.
    private var cue: Double = 0
    private var resting = false

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
        link?.invalidate()
        link = nil
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(step(_:)))
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 120)
        link.add(to: .main, forMode: .common)
        self.link = link
        resized()
    }

    public override func setFrameSize(_ size: NSSize) {
        super.setFrameSize(size)
        resized()
    }

    public override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        resized()
    }

    private func resized() {
        guard let layer = layer as? CAMetalLayer else { return }
        let scale = window?.backingScaleFactor ?? layer.contentsScale
        let size = CGSize(width: max(bounds.width * scale, 1), height: max(bounds.height * scale, 1))
        layer.contentsScale = scale
        layer.drawableSize = size
        surface?.resize(width: Int(size.width), height: Int(size.height))
        resting = false
    }

    private func restart() {
        began = nil
        resting = false
        if let session, let state {
            cue = (try? session.duration(of: state)) ?? 0
        }
    }

    @objc private func step(_ link: CADisplayLink) {
        guard !resting, let session, let state, let layer = layer as? CAMetalLayer, bounds.width > 0 else { return }
        do {
            if surface == nil {
                let size = layer.drawableSize
                surface = try ScaenaSurface(layer: layer, width: Int(size.width), height: Int(size.height))
            }
            let start = began ?? link.timestamp
            began = start
            let ms = (link.timestamp - start) * 1000
            let done = ms >= cue
            let shown = try surface?.paint(session, state: state, at: done ? .infinity : ms) ?? false
            resting = done && shown
            failure = nil
        } catch let error as ScaenaError {
            failure = error
            resting = true
        } catch {
            failure = ScaenaError(message: "\(error)")
            resting = true
        }
    }
}

/// `ScaenaView` in SwiftUI: `state` of `session`, its cue played each time it changes.
public struct ScaenaCanvas: NSViewRepresentable {
    public let session: ScaenaSession
    public let state: String

    public init(session: ScaenaSession, state: String) {
        self.session = session
        self.state = state
    }

    public func makeNSView(context: Context) -> ScaenaView {
        let view = ScaenaView(frame: .zero)
        view.session = session
        view.state = state
        return view
    }

    public func updateNSView(_ view: ScaenaView, context: Context) {
        if view.session !== session { view.session = session }
        if view.state != state { view.state = state }
    }
}
