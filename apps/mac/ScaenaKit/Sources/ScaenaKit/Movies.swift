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
        /// Its chapters, as the CLI's video carries them (SPEC §10): each beat as it plays, by its
        /// claim, and a slide no beat names by its headline; none where the deck has no spine.
        public let chapters: [Chapter]?
    }

    /// A part of the movie a player can skip to: its title, and where it starts and ends, ms in.
    public struct Chapter: Decodable, Sendable, Equatable {
        public let title: String
        public let start: Double
        public let end: Double
    }

    /// The bundle's file at `path`, as the session holds it: a movie's sound track among them.
    public func file(_ path: String) throws -> Data {
        var error: UnsafeMutablePointer<CChar>?
        return try Self.take(scaena_file(handle, path, &error), error)
    }

    /// The kinds of sound file a movie takes under its pictures, by their names' extensions.
    public static let soundKinds: Set<String> = ["aac", "aif", "aiff", "caf", "m4a", "mp3", "wav"]

    /// The bundle's sound files, by their paths in it: what a movie may take as its sound track,
    /// as the CLI's `--audio` takes a file.
    public func sounds() throws -> [String] {
        let files: [String] = try call("files")
        return files.filter { Self.soundKinds.contains(($0 as NSString).pathExtension.lowercased()) }.sorted()
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
/// as the canvas, made even as H.264 asks, the deck's `title` its title; a chapter for each beat as
/// it plays, as the CLI's video carries them, and a sound track under it where one is given.
enum MovieWriter {
    /// The frames a second a movie the editors export plays at, as the browser's Export… and the
    /// CLI's default make one.
    static let fps = 30

    /// `session`'s movie written to `url`, its first `seconds` alone where given, with `sound`, a
    /// sound file's bytes and its name, under it as AAC, cut where the deck's timeline ends; stopped
    /// between frames once `stopping` is asked, the file then taken away.
    static func write(
        _ session: ScaenaSession, to url: URL, title: String?, seconds: Double? = nil,
        sound: (data: Data, name: String)? = nil, stopping: Stopping
    ) throws {
        let movie = try session.movie(fps: fps)
        var frames = movie.frames
        if let seconds { frames = min(frames, max(1, Int((seconds * Double(fps)).rounded()))) }
        let end = CMTime(value: CMTimeValue(frames), timescale: CMTimeScale(fps))
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
        // The sound read from a file of its own name, which is how AVFoundation knows its kind.
        var soundFile: URL?
        defer { if let soundFile { try? FileManager.default.removeItem(at: soundFile) } }
        var input: AVAssetWriterInput?
        var adaptor: AVAssetWriterInputPixelBufferAdaptor?
        var feeds: [any Feeding] = []
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
                        let chapters = (movie.chapters ?? []).filter { $0.start < end.seconds * 1000 }
                        if !chapters.isEmpty {
                            let titles = try Chapters(chapters, ending: end, in: writer)
                            guard made.canAddTrackAssociation(withTrackOf: titles.input, type: AVAssetTrack.AssociationType.chapterList.rawValue)
                            else { throw ScaenaError(message: "the movie's chapters could not be joined to its pictures") }
                            made.addTrackAssociation(withTrackOf: titles.input, type: AVAssetTrack.AssociationType.chapterList.rawValue)
                            feeds.append(titles)
                        }
                        if let sound {
                            let file = FileManager.default.temporaryDirectory.appending(
                                path: "Scaena-\(UUID().uuidString).\((sound.name as NSString).pathExtension)")
                            try sound.data.write(to: file)
                            soundFile = file
                            feeds.append(try Sound(file, named: sound.name, ending: end, in: writer))
                        }
                        guard writer.startWriting() else { throw writer.error ?? ScaenaError(message: "the movie could not be begun") }
                        writer.startSession(atSourceTime: .zero)
                        input = made
                        adaptor = feeding
                    }
                }
                guard let input, let adaptor else { throw ScaenaError(message: "the movie's first frame was not painted") }
                let at = CMTime(value: CMTimeValue(k), timescale: CMTimeScale(fps))
                // The chapters and the sound written a second ahead of the pictures, and as far as
                // the writer wants them while it waits for theirs, so no track waits on another.
                for feed in feeds { try feed.feed(until: at + CMTime(value: 1, timescale: 1)) }
                while !input.isReadyForMoreMediaData {
                    if stopping.asked { throw CancellationError() }
                    for feed in feeds { try feed.feed(until: nil) }
                    Thread.sleep(forTimeInterval: 0.002)
                }
                let buffer = try Self.buffer(of: last, width: width, height: height, from: adaptor)
                guard adaptor.append(buffer, withPresentationTime: at)
                else { throw writer.error ?? ScaenaError(message: "frame \(k + 1) of the movie could not be written") }
            }
            input?.markAsFinished()
            while let open = feeds.first(where: { !$0.done }) {
                if stopping.asked { throw CancellationError() }
                try open.feed(until: nil)
                if !open.done { Thread.sleep(forTimeInterval: 0.002) }
            }
            // The last frame shown for its whole frame, so the movie runs `frames / fps` seconds,
            // and the sound cut there.
            writer.endSession(atSourceTime: end)
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

/// A track written beside the movie's pictures, a sample at a time as the writer is ready for one.
protocol Feeding: AnyObject {
    /// Whether every sample is written and the track marked finished.
    var done: Bool { get }
    /// Its samples that start before `until`, or all of them where none, written while the writer
    /// takes them: never waiting, so the pictures go on.
    func feed(until: CMTime?) throws
}

/// The movie's chapters as a QuickTime chapter track: a text sample for each, its title for as
/// long as it plays, which players list and skip to.
final class Chapters: Feeding {
    let input: AVAssetWriterInput
    private var samples: [CMSampleBuffer]
    private(set) var done = false

    init(_ chapters: [ScaenaSession.Chapter], ending end: CMTime, in writer: AVAssetWriter) throws {
        let format = try Self.format()
        input = AVAssetWriterInput(mediaType: .text, outputSettings: nil, sourceFormatHint: format)
        input.expectsMediaDataInRealTime = false
        input.marksOutputTrackAsEnabled = false
        input.languageCode = "und"
        guard writer.canAdd(input) else { throw ScaenaError(message: "the movie cannot hold its chapters") }
        writer.add(input)
        let last = end.seconds * 1000
        samples = try chapters.map { chapter in
            try Self.sample(chapter.title, from: chapter.start, to: min(chapter.end, last), format: format)
        }
    }

    func feed(until: CMTime?) throws {
        while !done, input.isReadyForMoreMediaData {
            guard let next = samples.first else {
                input.markAsFinished()
                done = true
                return
            }
            if let until, CMSampleBufferGetPresentationTimeStamp(next) >= until { return }
            guard input.append(next) else { throw ScaenaError(message: "a chapter of the movie could not be written") }
            samples.removeFirst()
        }
    }

    /// QuickTime text's sample description, as a chapter track's is written: black on nothing.
    private static func format() throws -> CMFormatDescription {
        let black: [String: Any] = [
            kCMTextFormatDescriptionColor_Red as String: 0, kCMTextFormatDescriptionColor_Green as String: 0,
            kCMTextFormatDescriptionColor_Blue as String: 0, kCMTextFormatDescriptionColor_Alpha as String: 255,
        ]
        let none: [String: Any] = [
            kCMTextFormatDescriptionRect_Top as String: 0, kCMTextFormatDescriptionRect_Left as String: 0,
            kCMTextFormatDescriptionRect_Bottom as String: 0, kCMTextFormatDescriptionRect_Right as String: 0,
        ]
        let style: [String: Any] = [
            kCMTextFormatDescriptionStyle_StartChar as String: 0,
            kCMTextFormatDescriptionStyle_Font as String: 1,
            kCMTextFormatDescriptionStyle_FontFace as String: 0,
            kCMTextFormatDescriptionStyle_ForegroundColor as String: black,
            kCMTextFormatDescriptionStyle_FontSize as String: 12,
        ]
        let extensions: [String: Any] = [
            kCMTextFormatDescriptionExtension_DisplayFlags as String: 0,
            kCMTextFormatDescriptionExtension_BackgroundColor as String: black,
            kCMTextFormatDescriptionExtension_DefaultTextBox as String: none,
            kCMTextFormatDescriptionExtension_DefaultStyle as String: style,
            kCMTextFormatDescriptionExtension_HorizontalJustification as String: 0,
            kCMTextFormatDescriptionExtension_VerticalJustification as String: 0,
            kCMTextFormatDescriptionExtension_FontTable as String: [NSNumber(value: 1): "Sans-Serif"] as NSDictionary,
        ]
        var made: CMFormatDescription?
        let status = CMFormatDescriptionCreate(
            allocator: kCFAllocatorDefault, mediaType: kCMMediaType_Text, mediaSubType: kCMTextFormatType_QTText,
            extensions: extensions as CFDictionary, formatDescriptionOut: &made)
        guard status == noErr, let made else { throw ScaenaError(message: "the movie's chapters could not be described") }
        return made
    }

    /// `title` from `start` to `end`, ms into the movie, as QuickTime text: its length in two bytes,
    /// big-endian, its UTF-8, then the atom that says it is UTF-8.
    private static func sample(_ title: String, from start: Double, to end: Double, format: CMFormatDescription)
        throws -> CMSampleBuffer
    {
        let words = Array(title.utf8.prefix(0xFFFF))
        let encoding: [UInt8] = [0, 0, 0, 12, 0x65, 0x6E, 0x63, 0x64, 0x08, 0x00, 0x01, 0x00]
        let bytes = [UInt8(words.count >> 8), UInt8(words.count & 0xFF)] + words + encoding
        var block: CMBlockBuffer?
        guard
            CMBlockBufferCreateWithMemoryBlock(
                allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: bytes.count,
                blockAllocator: kCFAllocatorDefault, customBlockSource: nil, offsetToData: 0,
                dataLength: bytes.count, flags: 0, blockBufferOut: &block) == noErr,
            let block,
            bytes.withUnsafeBytes({ raw in
                CMBlockBufferReplaceDataBytes(
                    with: raw.baseAddress!, blockBuffer: block, offsetIntoDestination: 0, dataLength: bytes.count)
            }) == noErr
        else { throw ScaenaError(message: "a chapter of the movie could not be written") }
        let from = CMTime(value: CMTimeValue(start.rounded()), timescale: 1000)
        let to = CMTime(value: CMTimeValue(max(end, start + 1).rounded()), timescale: 1000)
        var timing = CMSampleTimingInfo(duration: to - from, presentationTimeStamp: from, decodeTimeStamp: .invalid)
        var size = bytes.count
        var sample: CMSampleBuffer?
        guard
            CMSampleBufferCreate(
                allocator: kCFAllocatorDefault, dataBuffer: block, dataReady: true, makeDataReadyCallback: nil,
                refcon: nil, formatDescription: format, sampleCount: 1, sampleTimingEntryCount: 1,
                sampleTimingArray: &timing, sampleSizeEntryCount: 1, sampleSizeArray: &size,
                sampleBufferOut: &sample) == noErr,
            let sample
        else { throw ScaenaError(message: "a chapter of the movie could not be written") }
        return sample
    }
}

/// A sound file read as PCM and written under the pictures as AAC, as far as the movie runs.
final class Sound: Feeding {
    private let reader: AVAssetReader
    private let output: AVAssetReaderTrackOutput
    private let input: AVAssetWriterInput
    private let end: CMTime
    private var next: CMSampleBuffer?
    private(set) var done = false

    init(_ file: URL, named name: String, ending end: CMTime, in writer: AVAssetWriter) throws {
        let asset = AVURLAsset(url: file)
        guard let track = asset.tracks(withMediaType: .audio).first,
            let described = track.formatDescriptions.first,
            let basic = CMAudioFormatDescriptionGetStreamBasicDescription(described as! CMFormatDescription)?
                .pointee
        else { throw ScaenaError(message: "\(name) holds no sound a movie can take") }
        let channels = Int(basic.mChannelsPerFrame)
        guard (1...2).contains(channels) else {
            throw ScaenaError(message: "\(name) has \(channels) channels: a movie takes a sound of one or two")
        }
        reader = try AVAssetReader(asset: asset)
        output = AVAssetReaderTrackOutput(
            track: track,
            outputSettings: [
                AVFormatIDKey: kAudioFormatLinearPCM, AVLinearPCMBitDepthKey: 16, AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false, AVLinearPCMIsNonInterleaved: false,
            ])
        guard reader.canAdd(output) else { throw ScaenaError(message: "\(name) cannot be read as sound") }
        reader.add(output)
        input = AVAssetWriterInput(
            mediaType: .audio,
            outputSettings: [
                AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: min(max(basic.mSampleRate, 8000), 48000),
                AVNumberOfChannelsKey: channels, AVEncoderBitRateKey: 128_000,
            ])
        input.expectsMediaDataInRealTime = false
        guard writer.canAdd(input) else { throw ScaenaError(message: "the movie cannot hold \(name)'s sound") }
        writer.add(input)
        self.end = end
        guard reader.startReading() else { throw reader.error ?? ScaenaError(message: "\(name) cannot be read") }
    }

    func feed(until: CMTime?) throws {
        while !done, input.isReadyForMoreMediaData {
            if next == nil { next = output.copyNextSampleBuffer() }
            guard let sample = next, CMSampleBufferGetPresentationTimeStamp(sample) < end else {
                // The sound ended, or the movie does here: what is left of it is not heard.
                reader.cancelReading()
                input.markAsFinished()
                done = true
                return
            }
            if let until, CMSampleBufferGetPresentationTimeStamp(sample) >= until { return }
            guard input.append(sample) else { throw ScaenaError(message: "the movie's sound could not be written") }
            next = nil
        }
    }
}
