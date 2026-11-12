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

/// A movie's chapters are the CLI's video's (SPEC §10): each beat as it plays, titled by its claim,
/// written as a chapter track AVFoundation reads back with their titles and times.
@Test func aMoviesChaptersAreItsBeatsAsTheCLIsVideoCarriesThem() async throws {
    let session = try ScaenaSession(directory: b1)
    let movie = try session.movie(fps: 30)
    let chapters = try #require(movie.chapters)
    #expect(chapters.count > 2, "\(movie)")
    #expect(chapters.first?.start == 0 && chapters.last?.end == movie.durationMs)
    // The first two chapters and a part of the third: the third is cut where the movie ends.
    let seconds = (chapters[1].end + 500) / 1000
    let shown = chapters.filter { $0.start < seconds * 1000 }
    let data = try await session.snapshot().exported(.movie(title: "B1", seconds: seconds))
    let url = FileManager.default.temporaryDirectory.appending(path: "MovieTests-\(UUID().uuidString).mp4")
    try data.write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }

    let asset = AVURLAsset(url: url)
    let locales = try await asset.load(.availableChapterLocales)
    let groups = try await asset.loadChapterMetadataGroups(
        withTitleLocale: locales.first ?? Locale(identifier: "und"), containingItemsWithCommonKeys: [.commonKeyTitle])
    #expect(groups.count == shown.count, "\(groups.count) chapters read of \(shown.count)")
    for (group, chapter) in zip(groups, shown) {
        let title = try await group.items.first { $0.commonKey == .commonKeyTitle }?.load(.stringValue)
        #expect(title == chapter.title)
        #expect(abs(group.timeRange.start.seconds * 1000 - chapter.start) < 2, "\(chapter.title)")
    }
}

/// A sound file of the bundle goes under the movie as AAC, as the CLI's `--audio` takes one, cut
/// where the deck's timeline ends.
@Test func aMovieTakesABundlesSoundUnderItsPictures() async throws {
    let bundle = FileManager.default.temporaryDirectory.appending(path: "MovieTests-\(UUID().uuidString).scaena")
    try FileManager.default.copyItem(at: b1, to: bundle)
    defer { try? FileManager.default.removeItem(at: bundle) }
    try FileManager.default.createDirectory(at: bundle.appending(path: "sounds"), withIntermediateDirectories: true)
    try tone(seconds: 3).write(to: bundle.appending(path: "sounds/tone.wav"))
    let session = try ScaenaSession(directory: bundle)
    #expect(try session.sounds() == ["sounds/tone.wav"])

    let data = try await session.snapshot().exported(.movie(title: "B1", seconds: 1, sound: "sounds/tone.wav"))
    let url = FileManager.default.temporaryDirectory.appending(path: "MovieTests-\(UUID().uuidString).mp4")
    try data.write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }
    let asset = AVURLAsset(url: url)
    let duration = try await asset.load(.duration)
    #expect(abs(duration.seconds - 1) < 0.05, "it runs \(duration.seconds) s")
    let sound = try #require(try await asset.loadTracks(withMediaType: .audio).first, "the movie has a sound track")
    let heard = try await sound.load(.timeRange)
    #expect(heard.duration.seconds > 0.5 && heard.duration.seconds <= 1.1, "its sound runs \(heard.duration.seconds) s")
    #expect(try await asset.loadTracks(withMediaType: .video).count == 1)

    // A sound the bundle lacks is an error that names it.
    await #expect(throws: ScaenaError.self) {
        try await session.snapshot().exported(.movie(title: "B1", seconds: 1, sound: "sounds/none.wav"))
    }
}

/// A movie is made at the width and the frames a second its sheet asks, as the browser's Export…
/// offers them: 640 pixels wide in the canvas's proportions, at 24 frames a second.
@Test func aMovieIsMadeAtTheWidthAndFramesASecondAsked() async throws {
    let session = try ScaenaSession(directory: b1)
    let canvas = try session.canvasSize()
    let data = try await session.snapshot().exported(.movie(title: "B1", seconds: 1, width: 640, fps: 24))
    let url = FileManager.default.temporaryDirectory.appending(path: "MovieTests-\(UUID().uuidString).mp4")
    try data.write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }
    let asset = AVURLAsset(url: url)
    let duration = try await asset.load(.duration)
    #expect(abs(duration.seconds - 1) < 0.05, "it runs \(duration.seconds) s")
    let track = try #require(try await asset.loadTracks(withMediaType: .video).first)
    let size = try await track.load(.naturalSize)
    let asked = MovieSize.pixels(width: 640, canvas: canvas)
    #expect(Int(size.width) == 640 && abs(Int(size.height) - asked.height) <= 2, "\(size), \(asked)")
    let rate = try await track.load(.nominalFrameRate)
    #expect(abs(rate - 24) < 0.5, "it plays at \(rate) frames a second")
    // Every frame written, decoded as the first test reads them, a buffer that holds no picture
    // not counted.
    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(
        track: track, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
    reader.add(output)
    #expect(reader.startReading())
    var frames = 0
    while let sample = output.copyNextSampleBuffer() {
        if CMSampleBufferGetNumSamples(sample) > 0, CMSampleBufferGetImageBuffer(sample) != nil { frames += 1 }
    }
    #expect(frames == 24)

    // A rate the sheet does not offer, and a width a movie may not be: each an error that says why.
    await #expect(throws: ScaenaError.self) {
        try await session.snapshot().exported(.movie(title: "B1", seconds: 1, fps: 25))
    }
    await #expect(throws: ScaenaError.self) {
        try await session.snapshot().exported(.movie(title: "B1", seconds: 1, width: 9000))
    }
}

/// The sheet's width read as the browser's field reads it, and the pixels it makes: a whole number
/// from 2 to 8192, made even, the height the canvas's proportions give it.
@Test func aMoviesWidthIsReadAndItsHeightFollowsTheCanvas() {
    #expect(MovieSize.width("1280") == 1280 && MovieSize.width(" 640 ") == 640)
    #expect(MovieSize.width("1") == nil && MovieSize.width("9000") == nil)
    #expect(MovieSize.width("12.5") == nil && MovieSize.width("wide") == nil)
    #expect(MovieSize.rates == [24, 30, 60] && MovieSize.fps == 30)
    let made = MovieSize.pixels(width: 1281, canvas: CGSize(width: 1920, height: 1080))
    #expect(made.width == 1280 && made.height == 720, "\(made)")
    let tall = MovieSize.pixels(width: 1080, canvas: CGSize(width: 1080, height: 1920))
    #expect(tall.width == 1080 && tall.height == 1920, "\(tall)")
}

/// `seconds` of a 440 Hz tone as a WAV file: mono, 16-bit, 44.1 kHz.
private func tone(seconds: Double) -> Data {
    let rate = 44_100
    let count = Int(seconds * Double(rate))
    var made = Data()
    func put<T: FixedWidthInteger>(_ value: T) { withUnsafeBytes(of: value.littleEndian) { made.append(contentsOf: $0) } }
    made.append(contentsOf: Array("RIFF".utf8))
    put(UInt32(36 + count * 2))
    made.append(contentsOf: Array("WAVEfmt ".utf8))
    put(UInt32(16))
    put(UInt16(1))
    put(UInt16(1))
    put(UInt32(rate))
    put(UInt32(rate * 2))
    put(UInt16(2))
    put(UInt16(16))
    made.append(contentsOf: Array("data".utf8))
    put(UInt32(count * 2))
    for i in 0..<count {
        put(Int16(sin(2 * Double.pi * 440 * Double(i) / Double(rate)) * 8000))
    }
    return made
}
