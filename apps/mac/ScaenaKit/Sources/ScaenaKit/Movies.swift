import AVFoundation
import CScaena
import CoreVideo
import Foundation
import os

/// The deck as a movie on the Mac and the iPad (lift-up 20261031 FUNC-2), as the browser's Export…
/// makes one: the global timeline as `scaena export --format mp4` plays it, each state's cue then
/// its hold, paced by the beats where a state has none, each frame painted by the session's CPU
/// painter and encoded by AVFoundation as H.264 in an MPEG-4 file, at the canvas's size.
extension ScaenaSession {
    /// The movie of a deck at so many frames a second: how many frames it has, and how long it runs.
    public struct Movie: Decodable, Sendable, Equatable {
        public let frames: Int
        public let fps: Int
        public let durationMs: Double
    }

    /// The deck's movie of `fps` frames a second, as the browser's `movie` gives it; thrown where
    /// it would run 0 ms or past a day, as the CLI refuses one.
    public func movie(fps: Int) throws -> Movie {
        try call("movie", ["fps": .number(Double(fps))])
    }

    /// One frame of the movie: its drawing's digest, and its pixels, or none where the drawing is
    /// the frame's the caller has already.
    public struct MovieFrame: Sendable {
        public let digest: String
        public let pixels: Pixels?
    }

    /// Frame `k` of the movie of `fps` frames a second, painted `width` pixels wide by the CPU
    /// painter, unless its digest is `before`, the frame the caller has: the pixels the browser's
    /// `movieFrame` gives, so a hold with nothing moving is painted once.
    public func movieFrame(_ k: Int, fps: Int, width: Int, before: String? = nil) throws -> MovieFrame {
        var digest: UnsafeMutablePointer<CChar>?
        var error: UnsafeMutablePointer<CChar>?
        let painted: ScaenaPixels
        // The digest held, as a C string only where there is one.
        if let before {
            painted = scaena_movie_frame(handle, UInt32(fps), UInt64(k), UInt32(width), before, &digest, &error)
        } else {
            painted = scaena_movie_frame(handle, UInt32(fps), UInt64(k), UInt32(width), nil, &digest, &error)
        }
        guard let digest else { throw ScaenaError.taking(error) }
        let said = String(cString: digest)
        scaena_string_free(digest)
        guard painted.bytes.data != nil else {
            // Nothing painted and nothing wrong: the frame draws as `before` did.
            if let error { throw ScaenaError.taking(error) }
            return MovieFrame(digest: said, pixels: nil)
        }
        let rgba = try Self.take(painted.bytes, error)
        return MovieFrame(digest: said, pixels: Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height)))
    }
}

/// A cancel read from the thread a movie is made on, which no task's cancel reaches (lift-up
/// PERF-3): set by the task that waits on it, read between frames.
final class Stopping: Sendable {
    private let stopped = OSAllocatedUnfairLock(initialState: false)

    func stop() { stopped.withLock { $0 = true } }

    var asked: Bool { stopped.withLock { $0 } }
}

/// The session's movie written to a file: H.264 in an MPEG-4 file, `fps` frames a second, as wide
/// as the canvas, made even as H.264 asks, the deck's `title` its title.
enum MovieWriter {
    /// The frames a second a movie the editors export plays at, as the browser's Export… and the
    /// CLI's default make one.
    static let fps = 30

    /// `session`'s movie written to `url`, its first `seconds` alone where given, stopped between
    /// frames once `stopping` is asked, the file then taken away.
    static func write(
        _ session: ScaenaSession, to url: URL, title: String?, seconds: Double? = nil, stopping: Stopping
    ) throws {
        let movie = try session.movie(fps: fps)
        var frames = movie.frames
        if let seconds { frames = min(frames, max(1, Int((seconds * Double(fps)).rounded()))) }
        let canvas = try session.canvasSize()
        // H.264 draws an even number of pixels each way: the canvas's width made even, and its
        // height as the painter paints it, its last row dropped where odd.
        let width = max(2, Int(canvas.width) & ~1)
        try? FileManager.default.removeItem(at: url)
        let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        if let title, !title.isEmpty {
            let item = AVMutableMetadataItem()
            item.identifier = .commonIdentifierTitle
            item.value = title as NSString
            item.extendedLanguageTag = "und"
            writer.metadata = [item]
        }
        var input: AVAssetWriterInput?
        var adaptor: AVAssetWriterInputPixelBufferAdaptor?
        var height = 0
        var digest: String?
        var last = Data()
        do {
            for k in 0..<frames {
                if stopping.asked { throw CancellationError() }
                let frame = try session.movieFrame(k, fps: fps, width: width, before: digest)
                digest = frame.digest
                if let pixels = frame.pixels {
                    last = pixels.rgba
                    if input == nil {
                        // The writer is started once the first frame says how high the movie is.
                        height = max(2, pixels.height & ~1)
                        let made = AVAssetWriterInput(
                            mediaType: .video,
                            outputSettings: [
                                AVVideoCodecKey: AVVideoCodecType.h264,
                                AVVideoWidthKey: width,
                                AVVideoHeightKey: height,
                            ])
                        made.expectsMediaDataInRealTime = false
                        let feeding = AVAssetWriterInputPixelBufferAdaptor(
                            assetWriterInput: made,
                            sourcePixelBufferAttributes: [
                                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
                                kCVPixelBufferWidthKey as String: width,
                                kCVPixelBufferHeightKey as String: height,
                            ])
                        writer.add(made)
                        guard writer.startWriting() else { throw writer.error ?? ScaenaError(message: "the movie could not be begun") }
                        writer.startSession(atSourceTime: .zero)
                        input = made
                        adaptor = feeding
                    }
                }
                guard let input, let adaptor else { throw ScaenaError(message: "the movie's first frame was not painted") }
                while !input.isReadyForMoreMediaData {
                    if stopping.asked { throw CancellationError() }
                    Thread.sleep(forTimeInterval: 0.002)
                }
                let buffer = try Self.buffer(of: last, width: width, height: height, from: adaptor)
                guard adaptor.append(buffer, withPresentationTime: CMTime(value: CMTimeValue(k), timescale: CMTimeScale(fps)))
                else { throw writer.error ?? ScaenaError(message: "frame \(k + 1) of the movie could not be written") }
            }
            input?.markAsFinished()
            // The last frame shown for its whole frame, so the movie runs `frames / fps` seconds.
            writer.endSession(atSourceTime: CMTime(value: CMTimeValue(frames), timescale: CMTimeScale(fps)))
            let done = DispatchSemaphore(value: 0)
            writer.finishWriting { done.signal() }
            done.wait()
            guard writer.status == .completed else {
                throw writer.error ?? ScaenaError(message: "the movie could not be finished")
            }
        } catch {
            if writer.status == .writing { writer.cancelWriting() }
            try? FileManager.default.removeItem(at: url)
            throw error
        }
    }

    /// `rgba`, a frame `width` pixels wide painted straight-alpha, as the BGRA pixel buffer the
    /// encoder takes, `height` rows of it.
    private static func buffer(
        of rgba: Data, width: Int, height: Int, from adaptor: AVAssetWriterInputPixelBufferAdaptor
    ) throws -> CVPixelBuffer {
        var made: CVPixelBuffer?
        if let pool = adaptor.pixelBufferPool {
            CVPixelBufferPoolCreatePixelBuffer(nil, pool, &made)
        } else {
            CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_32BGRA, nil, &made)
        }
        guard let buffer = made else { throw ScaenaError(message: "no room for a frame of the movie") }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else {
            throw ScaenaError(message: "no room for a frame of the movie")
        }
        let stride = CVPixelBufferGetBytesPerRow(buffer)
        let into = base.assumingMemoryBound(to: UInt8.self)
        let rows = min(height, rgba.count / (width * 4))
        rgba.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            guard let from = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return }
            for y in 0..<rows {
                let source = from + y * width * 4
                let row = into + y * stride
                for x in 0..<width {
                    let p = x * 4
                    row[p] = source[p + 2]
                    row[p + 1] = source[p + 1]
                    row[p + 2] = source[p]
                    row[p + 3] = 255
                }
            }
        }
        return buffer
    }
}
