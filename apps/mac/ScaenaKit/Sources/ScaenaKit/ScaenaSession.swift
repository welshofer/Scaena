import CScaena
import Foundation

/// Why the engine stopped: what it was given is not what it takes, or what it needs is built
/// by a later PLAN task (`plan`), or a patch's op `op` failed.
public struct ScaenaError: Error, Decodable, Sendable, CustomStringConvertible, LocalizedError {
    public let message: String
    public let plan: String?
    public let op: Int?

    public var description: String { message }
    /// What an alert says: why a deck did not open, say, rather than that an operation failed.
    public var errorDescription: String? { message }

    init(message: String) {
        self.message = message
        self.plan = nil
        self.op = nil
    }

    /// The error the library wrote at `error`, freed.
    static func taking(_ error: UnsafeMutablePointer<CChar>?) -> ScaenaError {
        guard let error else { return ScaenaError(message: "the engine said nothing") }
        defer { scaena_string_free(error) }
        let data = Data(bytes: error, count: strlen(error))
        return (try? JSONDecoder().decode(ScaenaError.self, from: data))
            ?? ScaenaError(message: String(decoding: data, as: UTF8.self))
    }
}

/// One bundle open for editing: the session the browser's editor keeps, through the C ABI
/// (`scaena-ffi`, ADR-0021). Its calls answer as a page's `Player` does, so a gesture makes the
/// same patch here as there. Use it from one thread at a time.
public final class ScaenaSession {
    let handle: OpaquePointer

    /// Open a bundle from its files, each by its path in it (`deck.json`, `fonts/…`), as a page
    /// opens a folder.
    public init(files: [String: Data]) throws {
        guard let gathered = scaena_files_new() else { throw ScaenaError(message: "no room for the files") }
        for (path, data) in files {
            let added = data.withUnsafeBytes { raw in
                scaena_files_add(gathered, path, raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count)
            }
            if !added {
                scaena_files_free(gathered)
                throw ScaenaError(message: "\(path) could not be handed over")
            }
        }
        var error: UnsafeMutablePointer<CChar>?
        guard let opened = scaena_open(gathered, &error) else { throw ScaenaError.taking(error) }
        handle = opened
    }

    /// Open the bundle in `directory`: every file under it, hidden ones aside.
    public convenience init(directory: URL) throws {
        let root = directory.resolvingSymlinksInPath()
        let depth = root.pathComponents.count
        guard let walk = FileManager.default.enumerator(
            at: root, includingPropertiesForKeys: [.isRegularFileKey], options: [.skipsHiddenFiles])
        else { throw ScaenaError(message: "\(root.path) cannot be read") }
        var files: [String: Data] = [:]
        for case let url as URL in walk {
            guard try url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile == true else { continue }
            let path = url.resolvingSymlinksInPath().pathComponents.dropFirst(depth).joined(separator: "/")
            files[path] = try Data(contentsOf: url)
        }
        try self.init(files: files)
    }

    init(handle: OpaquePointer) {
        self.handle = handle
    }

    /// A new deck, as New makes one in the browser (PLAN 2.12, 3.3): the theme that ships as
    /// `theme` (`dusk`, `daybreak`, `ember`, or `salon`), its fonts, and one state with nothing on it,
    /// titled `title`. Kept nowhere until it is saved.
    public static func create(theme: String, title: String) throws -> ScaenaSession {
        var error: UnsafeMutablePointer<CChar>?
        guard let made = scaena_create(theme, title, &error) else { throw ScaenaError.taking(error) }
        return ScaenaSession(handle: made)
    }

    /// The themes that ship, in the order New tries them: Dusk first, as the browser's New starts.
    public static let newThemes = ["dusk", "daybreak", "ember", "salon"]

    /// A new deck, as `create` makes one, in the first of `themes` that makes one, with why each
    /// before it made none (lift-up STAB-3): New falls back to another theme that ships, and says
    /// so, rather than ending the app. Nil where none of them makes a deck.
    public static func create(
        first themes: [String] = newThemes, title: String
    ) -> (session: ScaenaSession?, failed: [(theme: String, error: any Error)]) {
        var failed: [(theme: String, error: any Error)] = []
        for theme in themes {
            do {
                return (try create(theme: theme, title: title), failed)
            } catch {
                failed.append((theme, error))
            }
        }
        return (nil, failed)
    }

    /// Open a `.scaena` zip.
    public init(zip: Data) throws {
        var error: UnsafeMutablePointer<CChar>?
        let opened = zip.withUnsafeBytes { raw in
            scaena_open_zip(raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count, &error)
        }
        guard let opened else { throw ScaenaError.taking(error) }
        handle = opened
    }

    deinit {
        scaena_session_free(handle)
    }

    /// Hand over a file by its path in the bundle: an image dropped, a font, data.
    public func add(_ data: Data, at path: String) throws {
        let added = data.withUnsafeBytes { raw in
            scaena_add_file(handle, path, raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count)
        }
        if !added { throw ScaenaError(message: "\(path) could not be handed over") }
    }

    /// The session's answer to `method`, as a page's `Player` gives it, decoded as `T`.
    public func call<T: Decodable>(_ method: String, _ args: JSONValue = nil, as type: T.Type = T.self) throws -> T {
        var json: String?
        if args != .null {
            json = String(decoding: try JSONEncoder().encode(args), as: UTF8.self)
        }
        return try Self.decode(scaena_call(handle, method, json), as: T.self)
    }

    /// What an MCP operation gave back, and whether it changed the deck.
    public struct Called: Sendable {
        public let result: JSONValue
        public let edited: Bool
    }

    /// Run the MCP server's operation `name` on the bundle (`deck_patch`, `deck_lint`,
    /// `deck_inspect`, …), its tool's arguments less `bundle`, `out`, and `painter`, as
    /// `author` at `date`: what a gesture on the canvas ends in.
    public func tool(_ name: String, _ args: JSONValue = nil, author: String = "user", at date: Date = Date()) throws -> Called {
        let json = String(decoding: try JSONEncoder().encode(args), as: UTF8.self)
        guard let answer = scaena_tool(handle, name, json, author, date.formatted(.iso8601)) else {
            throw ScaenaError(message: "the engine said nothing")
        }
        let data = Data(bytes: answer, count: strlen(answer))
        scaena_string_free(answer)
        let called = try JSONDecoder().decode(ToolAnswer.self, from: data)
        if let error = called.error { throw error }
        return Called(result: called.ok ?? .null, edited: called.edited ?? false)
    }

    /// The deck's states, in order.
    public func states() throws -> [String] { try call("states") }

    /// The deck's title, where its `meta` gives one.
    public func title() throws -> String? { try call("title") }

    /// The canvas frames are laid out on, in canvas units: the deck's, or its format's.
    public func canvasSize() throws -> CGSize {
        let size: [Double] = try call("canvasSize")
        guard size.count == 2 else { throw ScaenaError(message: "a canvas has a width and a height") }
        return CGSize(width: size[0], height: size[1])
    }

    /// A state on the deck's timeline: where it starts, its cue's span, and its hold, ms; and the
    /// words that head it, where a text does, which the slides down the side are read by (lift-up
    /// 10-24b USE-4).
    public struct Slot: Decodable, Equatable, Sendable {
        public let state: String
        public let slide: String
        public let start: Double
        public let span: Double
        public let hold: Double
        public var headline: String? = nil
    }

    /// The deck's states end to end (SPEC §2.4).
    public func timeline() throws -> [Slot] { try call("timeline") }

    /// How long `state`'s cue runs, ms. Past it, the state is at rest.
    public func duration(of state: String) throws -> Double { try call("duration", ["state": .string(state)]) }

    /// Lay out the cue into `state` ahead, while the state shown rests: the state a show goes on
    /// to, whose first frame then lays out nothing (lift-up PERF-1).
    public func prepare(_ state: String) throws {
        let _: JSONValue = try call("prepare", ["state": .string(state)])
    }

    /// The deck as canonical `.scn`.
    public func source() throws -> String { try call("source") }

    /// `state`'s display list at `ms` into its cue (infinity: at rest), postcard-encoded (SPEC §6).
    public func frame(_ state: String, at ms: Double = .infinity) throws -> Data {
        var error: UnsafeMutablePointer<CChar>?
        return try Self.take(scaena_frame(handle, state, ms, &error), error)
    }

    /// A frame painted: straight-alpha sRGB, four bytes a pixel, row by row.
    public struct Pixels: Sendable {
        public let rgba: Data
        public let width: Int
        public let height: Int
    }

    /// `state` at `ms` (infinity: at rest), painted by the CPU painter `width` pixels wide.
    public func pixels(_ state: String, at ms: Double = .infinity, width: Int) throws -> Pixels {
        var error: UnsafeMutablePointer<CChar>?
        let painted = scaena_pixels(handle, state, ms, UInt32(width), &error)
        let rgba = try Self.take(painted.bytes, error)
        return Pixels(rgba: rgba, width: Int(painted.width), height: Int(painted.height))
    }

    /// The deck as a PDF (PLAN 3.8): a page for each slide at its last state, the bytes `scaena
    /// export --format pdf` writes, as the browser's PDF module draws them; with `notes`, each
    /// slide smaller with the presenter's notes under it, the bytes `--notes` writes.
    public func pdf(notes: Bool = false) throws -> Data {
        let asked: JSONValue = ["notes": .bool(notes)]
        let args = String(decoding: try JSONEncoder().encode(asked), as: UTF8.self)
        var error: UnsafeMutablePointer<CChar>?
        return try Self.take(scaena_export(handle, "pdf", args, &error), error)
    }

    /// `state` at rest as a PNG `width` pixels wide, painted by the CPU painter, as the editor's
    /// PNG export paints it.
    public func png(_ state: String, width: Int) throws -> Data {
        let asked: JSONValue = ["state": .string(state), "width": .number(Double(width))]
        let args = String(decoding: try JSONEncoder().encode(asked), as: UTF8.self)
        var error: UnsafeMutablePointer<CChar>?
        return try Self.take(scaena_export(handle, "png", args, &error), error)
    }

    /// Every slide once its steps have played, as PNGs `width` pixels wide in one zip, each named
    /// by its number, `name - Slide 3.png` (PLAN 2.122), as the editor's Export makes it.
    public func slides(width: Int, name: String) throws -> Data {
        let asked: JSONValue = ["width": .number(Double(width)), "name": .string(name)]
        let args = String(decoding: try JSONEncoder().encode(asked), as: UTF8.self)
        var error: UnsafeMutablePointer<CChar>?
        return try Self.take(scaena_export(handle, "slides", args, &error), error)
    }

    /// A save: the bundle's files as saved, to write where it is kept, then to adopt. Sendable: a
    /// save of a snapshot is made on another thread and adopted on the session's, one thread at a
    /// time (lift-up PERF-5).
    public final class Saved: @unchecked Sendable {
        fileprivate let handle: OpaquePointer
        /// Every file of the saved bundle, by its path in it.
        public let files: [String]
        /// The files of the bundle as it was that the save renamed or rewrote: one saved in
        /// place drops those `files` does not hold.
        public let replaced: [String]
        /// What the save did: `{ renamed, subset, … }`.
        public let summary: JSONValue

        fileprivate init(handle: OpaquePointer, listed: SavedList) {
            self.handle = handle
            self.files = listed.files
            self.replaced = listed.replaced
            self.summary = listed.summary
        }

        deinit {
            scaena_saved_free(handle)
        }

        /// The saved file at `path`, or none.
        public func file(_ path: String) -> Data? {
            let bytes = scaena_saved_file(handle, path)
            guard let data = bytes.data else { return nil }
            defer { scaena_bytes_free(bytes) }
            return Data(bytes: data, count: bytes.len)
        }

        /// The saved bundle as one `.scaena` zip.
        public func zip() throws -> Data {
            var error: UnsafeMutablePointer<CChar>?
            return try ScaenaSession.take(scaena_saved_zip(handle, &error), error)
        }
    }

    /// Save the bundle at `date` as `scaena save` lays one out: files named by their content,
    /// fonts subset if `subset`, and the edits since recorded in its history, where it keeps one.
    public func save(at date: Date = Date(), subset: Bool = true) throws -> Saved {
        var error: UnsafeMutablePointer<CChar>?
        guard let saved = scaena_save(handle, date.formatted(.iso8601), subset, &error) else {
            throw ScaenaError.taking(error)
        }
        do {
            let listed = try Self.decode(scaena_saved_list(saved), as: SavedList.self)
            return Saved(handle: saved, listed: listed)
        } catch {
            scaena_saved_free(saved)
            throw error
        }
    }

    /// Go on from `saved`, once it is written where the bundle is kept: its files are the
    /// session's from now on.
    public func adopt(_ saved: Saved) throws {
        _ = try Self.decode(scaena_adopt(handle, saved.handle), as: JSONValue.self)
    }

    /// What a save reads of the session, taken now (lift-up PERF-5): saved on another thread
    /// while the session goes on, then adopted with `adopt(_:since:)`. Sendable: it is taken on the
    /// session's thread, saved on one other, and handed back after, one thread at a time.
    public final class Snapshot: @unchecked Sendable {
        fileprivate let handle: OpaquePointer

        fileprivate init(handle: OpaquePointer) {
            self.handle = handle
        }

        deinit {
            scaena_snapshot_free(handle)
        }

        /// Saved at `date` as the session's `save` saves it: fonts subset if `subset`, the edits
        /// since recorded in its history, on the thread that calls it, which touches no session.
        public func save(at date: Date = Date(), subset: Bool = true) throws -> Saved {
            var error: UnsafeMutablePointer<CChar>?
            guard let saved = scaena_snapshot_save(handle, date.formatted(.iso8601), subset, &error) else {
                throw ScaenaError.taking(error)
            }
            do {
                let listed = try ScaenaSession.decode(scaena_saved_list(saved), as: SavedList.self)
                return Saved(handle: saved, listed: listed)
            } catch {
                scaena_saved_free(saved)
                throw error
            }
        }

        /// A session of its own on the deck and files as the snapshot was taken, to run a call
        /// that only reads on the thread that holds it (narrative lift-up PERF-2,
        /// `ScaenaChat.runApart`): it answers as the session it was taken from did.
        public func opened() throws -> ScaenaSession {
            var error: UnsafeMutablePointer<CChar>?
            guard let session = scaena_snapshot_open(handle, &error) else { throw ScaenaError.taking(error) }
            return ScaenaSession(handle: session)
        }
    }

    /// What a save reads of the session as it stands, to be saved away from its thread.
    public func snapshot() throws -> Snapshot {
        guard let taken = scaena_snapshot_new(handle) else { throw ScaenaError(message: "the engine took no snapshot") }
        return Snapshot(handle: taken)
    }

    /// Go on from `saved`, a save of `snapshot`, once it is written where the bundle is kept, its
    /// files taken out of it, where the session is still as `snapshot` was taken (lift-up PERF-5).
    /// Whether it did: an edit made while the save was written is kept, nothing adopted, and the
    /// next save names its files by their content again.
    @discardableResult
    public func adopt(_ saved: Saved, since snapshot: Snapshot) throws -> Bool {
        struct Adopted: Decodable {
            let adopted: Bool
        }
        return try Self.decode(scaena_adopt_since(handle, snapshot.handle, saved.handle), as: Adopted.self).adopted
    }

    /// The `ok` of the envelope at `answer`, freed, or its `error` thrown.
    static func decode<T: Decodable>(_ answer: UnsafeMutablePointer<CChar>?, as type: T.Type) throws -> T {
        guard let answer else { throw ScaenaError(message: "the engine said nothing") }
        let data = Data(bytes: answer, count: strlen(answer))
        scaena_string_free(answer)
        switch try JSONDecoder().decode(Envelope<T>.self, from: data).result {
        case .success(let value): return value
        case .failure(let error): throw error
        }
    }

    /// `bytes` as data, freed, or the error at `error` thrown where there are none.
    static func take(_ bytes: ScaenaBytes, _ error: UnsafeMutablePointer<CChar>?) throws -> Data {
        guard let data = bytes.data else { throw ScaenaError.taking(error) }
        defer { scaena_bytes_free(bytes) }
        return Data(bytes: data, count: bytes.len)
    }
}

/// `{"ok": value}` or `{"error": {...}}`.
private struct Envelope<T: Decodable>: Decodable {
    let result: Result<T, ScaenaError>

    private enum Keys: String, CodingKey { case ok, error }

    init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        if let error = try fields.decodeIfPresent(ScaenaError.self, forKey: .error) {
            result = .failure(error)
        } else {
            result = .success(try fields.decode(T.self, forKey: .ok))
        }
    }
}

/// What an MCP operation returns: `{"ok", "edited"}` or `{"error"}`.
private struct ToolAnswer: Decodable {
    let ok: JSONValue?
    let edited: Bool?
    let error: ScaenaError?
}

/// What a save holds, as `scaena_saved_list` says it.
fileprivate struct SavedList: Decodable {
    let files: [String]
    let replaced: [String]
    let summary: JSONValue
}
