import CScaena
import Foundation
import QuartzCore

/// The engine's frames on a `CAMetalLayer`, painted by vello on Metal and presented at the
/// display's refresh (PLAN 3.2): what `ScaenaView` shows. Use it from the thread that owns the
/// layer.
public final class ScaenaSurface {
    private let handle: OpaquePointer

    /// Make the GPU every surface paints with, if it is not made yet: its device and vello's
    /// pipelines, most of what a first frame costs. The app does it as it starts, off the main
    /// thread, so that the first deck it opens shows its first frame without waiting for it
    /// (gate 3). A surface made meanwhile waits for it.
    public static func warm() throws {
        var error: UnsafeMutablePointer<CChar>?
        guard scaena_gpu_warm(&error) else { throw ScaenaError.taking(error) }
    }

    /// Paint on `layer`, `width` × `height` device pixels. The layer keeps the deck's aspect, and
    /// outlives the surface.
    public init(layer: CAMetalLayer, width: Int, height: Int) throws {
        var error: UnsafeMutablePointer<CChar>?
        let pointer = Unmanaged.passUnretained(layer).toOpaque()
        guard let made = scaena_surface_new(pointer, UInt32(max(width, 1)), UInt32(max(height, 1)), &error) else {
            throw ScaenaError.taking(error)
        }
        handle = made
    }

    deinit {
        scaena_surface_free(handle)
    }

    /// Paint at `width` × `height` device pixels from now on.
    public func resize(width: Int, height: Int) {
        _ = scaena_surface_resize(handle, UInt32(max(width, 1)), UInt32(max(height, 1)))
    }

    /// Paint `state` at `ms` into its cue (infinity: at rest) from `session`, and present it at
    /// the next refresh. False where the layer had no drawable to give: the frame is skipped.
    @discardableResult
    public func paint(_ session: ScaenaSession, state: String, at ms: Double = .infinity) throws -> Bool {
        var error: UnsafeMutablePointer<CChar>?
        switch scaena_surface_paint(handle, session.handle, state, ms, &error) {
        case 1: return true
        case 0: return false
        default: throw ScaenaError.taking(error)
        }
    }

    /// The last frame painted, read back as the layer was given it.
    public func lastFrame() throws -> ScaenaSession.Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let read = scaena_surface_pixels(handle, &error)
        let rgba = try ScaenaSession.take(read.bytes, error)
        return ScaenaSession.Pixels(rgba: rgba, width: Int(read.width), height: Int(read.height))
    }

    /// The adapter that paints, and what paints on it: `{ name, backend, device, painter }`, the
    /// painter `vello`, or `cpu` where the GPU runs no vello (the iPad simulator's, PLAN 4.1).
    public func adapter() throws -> JSONValue {
        try ScaenaSession.decode(scaena_surface_adapter(handle), as: JSONValue.self)
    }
}
