import CScaena
import Foundation

/// Why the engine stopped: what it was given is not what it takes, or what it needs is built
/// by a later PLAN task (`plan`), or a patch's op `op` failed.
public struct ScaenaError: Error, Decodable, Sendable, CustomStringConvertible {
    public let message: String
    public let plan: String?
    public let op: Int?

    public var description: String { message }

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
    private let handle: OpaquePointer

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

    /// A state on the deck's timeline: where it starts, its cue's span, and its hold, ms.
    public struct Slot: Decodable, Equatable, Sendable {
        public let state: String
        public let slide: String
        public let start: Double
        public let span: Double
        public let hold: Double
    }

    /// The deck's states end to end (SPEC §2.4).
    public func timeline() throws -> [Slot] { try call("timeline") }

    /// How long `state`'s cue runs, ms. Past it, the state is at rest.
    public func duration(of state: String) throws -> Double { try call("duration", ["state": .string(state)]) }

    /// The deck as canonical `.scn`.
    public func source() throws -> String { try call("source") }

    /// Compile `source`: `{ error?, findings, states, valid }`. A deck that validates is what
    /// frames show from then on.
    public func compile(_ source: String) throws -> JSONValue { try call("compile", ["source": .string(source)]) }

    /// Lint the deck compiled last: `{ findings, laid, whole }`; with `state`, the layout rules
    /// run on that state alone.
    public func lint(state: String? = nil) throws -> JSONValue {
        guard let state else { return try call("lint") }
        return try call("lint", ["state": .string(state)])
    }

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

    /// A save: the bundle's files as saved, to write where it is kept, then to adopt.
    public final class Saved {
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

    /// The `ok` of the envelope at `answer`, freed, or its `error` thrown.
    private static func decode<T: Decodable>(_ answer: UnsafeMutablePointer<CChar>?, as type: T.Type) throws -> T {
        guard let answer else { throw ScaenaError(message: "the engine said nothing") }
        let data = Data(bytes: answer, count: strlen(answer))
        scaena_string_free(answer)
        switch try JSONDecoder().decode(Envelope<T>.self, from: data).result {
        case .success(let value): return value
        case .failure(let error): throw error
        }
    }

    /// `bytes` as data, freed, or the error at `error` thrown where there are none.
    fileprivate static func take(_ bytes: ScaenaBytes, _ error: UnsafeMutablePointer<CChar>?) throws -> Data {
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
