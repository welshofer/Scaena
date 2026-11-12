import AVFoundation
import Foundation
import ScaenaKit
import Testing

/// The deck as a movie on the Mac and the iPad (lift-up 20261031 FUNC-2): B1's first second
/// made away from the window as the PDF is, H.264 in an MPEG-4 file at the canvas's size and 30
/// frames a second, titled as the deck is, and read back by AVFoundation.
@Test func theDecksMovieIsWrittenAsTheSessionPlaysIt() async throws {
    let session = try ScaenaSession(directory: b1)
    let movie = try session.movie(fps: 30)
    #expect(movie.fps == 30 && movie.frames > 30 && movie.durationMs > 1000, "\(movie)")

    let data = try await session.snapshot().exported(.movie(title: "B1, as it plays", seconds: 1))
    let url = FileManager.default.temporaryDirectory.appending(path: "MovieTests-\(UUID().uuidString).mp4")
    try data.write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }

    let asset = AVURLAsset(url: url)
    let duration = try await asset.load(.duration)
    #expect(abs(duration.seconds - 1) < 0.05, "it runs \(duration.seconds) s")
    let track = try #require(try await asset.loadTracks(withMediaType: .video).first)
    let size = try await track.load(.naturalSize)
    let canvas = try session.canvasSize()
    #expect(Int(size.width) == Int(canvas.width) & ~1, "\(size) on a canvas \(canvas)")
    #expect(abs(Double(size.height) - canvas.height * Double(size.width) / canvas.width) <= 2, "\(size)")
    let title = try await asset.load(.commonMetadata).first { $0.commonKey == .commonKeyTitle }
    #expect(try await title?.load(.stringValue) == "B1, as it plays")

    // Every frame written, a hold's as each of the others.
    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(
        track: track, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
    reader.add(output)
    #expect(reader.startReading())
    var frames = 0
    while output.copyNextSampleBuffer() != nil { frames += 1 }
    #expect(frames == 30)
}

/// A frame of the movie is the session's drawing of the timeline at that moment, painted by the
/// CPU painter: where it is the first state at rest, its pixels are that state's, byte for byte;
/// and a frame that draws as the one before it is not painted again.
@Test func aMoviesFrameIsTheCPUPaintersPixelsForItsState() throws {
    let session = try ScaenaSession(directory: b1)
    let movie = try session.movie(fps: 30)
    let first = try #require(try session.states().first)
    let atRest = try session.digest(state: first)
    var k = 0
    var before: String?
    var found: ScaenaSession.MovieFrame?
    while k < movie.frames {
        let frame = try session.movieFrame(k, fps: 30, width: 320, before: before)
        if frame.digest == atRest {
            found = frame.pixels == nil ? try session.movieFrame(k, fps: 30, width: 320) : frame
            break
        }
        before = frame.digest
        k += 1
    }
    let frame = try #require(found, "the movie shows \(first) at rest")
    let pixels = try #require(frame.pixels)
    let painted = try session.pixels(first, width: 320)
    #expect(pixels.width == painted.width && pixels.height == painted.height)
    #expect(pixels.rgba == painted.rgba)
    // The same frame, given its digest: its pixels are the caller's already.
    #expect(try session.movieFrame(k, fps: 30, width: 320, before: frame.digest).pixels == nil)
    // A frame past the last, and a movie of no frames a second: each an error that says why.
    #expect(throws: ScaenaError.self) { try session.movieFrame(movie.frames, fps: 30, width: 320) }
    #expect(throws: ScaenaError.self) { try session.movie(fps: 0) }
}

/// A movie cancelled while it is made stops between its frames and throws, no file left behind.
@Test func aMovieCancelledStopsBetweenItsFrames() async throws {
    let session = try ScaenaSession(directory: b1)
    let snapshot = try session.snapshot()
    let making = Task { try await snapshot.exported(.movie(title: "B1")) }
    try await Task.sleep(for: .milliseconds(50))
    making.cancel()
    await #expect(throws: CancellationError.self) { try await making.value }
}
