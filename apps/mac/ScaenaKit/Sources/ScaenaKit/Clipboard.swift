import CScaena
import Foundation
import ImageIO
import UniformTypeIdentifiers

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The Edit menu's Copy, Cut, and Paste on the canvas (PLAN 3.12), on the Mac and the iPad.
public enum Clipping: Sendable {
    case copy, cut, paste
}

/// What a paste makes (PLAN 2.37): the copy of the node copied, the copies of the others copied
/// with it, the patch that adds them, the files they read that the bundle lacked, and what the
/// deck's theme lacked, each taken out of the copies.
public struct Pasted: Decodable, Sendable {
    public let id: String
    /// Its box once placed, `[x, y, width, height]`, canvas units.
    public let cell: [Double]
    public let also: [String]
    public let patch: [JSONValue]
    public let files: [String]
    /// What the theme lacked, as lint says it.
    public let lacked: [String]

    private enum Keys: String, CodingKey { case id, cell, also, patch, files, findings }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        id = try fields.decode(String.self, forKey: .id)
        cell = try fields.decode([Double].self, forKey: .cell)
        also = try fields.decodeIfPresent([String].self, forKey: .also) ?? []
        patch = try fields.decode([JSONValue].self, forKey: .patch)
        files = try fields.decodeIfPresent([String].self, forKey: .files) ?? []
        let findings = try fields.decodeIfPresent([JSONValue].self, forKey: .findings) ?? []
        lacked = findings.compactMap { $0["message"]?.string }
    }

    /// Every copy it makes, the copy of the node copied first.
    public var ids: [String] { [id] + also }
}

/// A look put down on nodes (PLAN 2.58): the patch of `choose`s, the nodes it changes, those that
/// look so already, and those that take none of it, each with why.
public struct Put: Decodable, Sendable {
    public let patch: [JSONValue]
    public let took: [String]
    public let same: [String]
    public let refused: [Refused]

    public struct Refused: Decodable, Sendable {
        public let node: String
        public let why: String
    }
}

/// A sheet's cells pasted (PLAN 2.96): the data source they would be, typed as they read.
public struct Cells: Decodable, Sendable {
    /// A name for the source and its file, made from the first columns' names.
    public let name: String
    public let columns: [String]
    /// Each column's type by its name, as the source declares it: what `attaching` takes.
    public let schema: JSONValue
    /// For each column, the format that prints its figures as they were copied, if any.
    public let formats: [String?]
    /// How many rows there are under the first.
    public let rows: Int
    /// The source's file: RFC 4180, the numbers written plainly.
    public let csv: String

    /// The table's columns, each printing its figures as they were copied: what a table of the
    /// cells inserted sets (`inserting`'s `with`).
    public var tableColumns: JSONValue {
        .array(
            zip(columns, formats).map { (field: String, format: String?) -> JSONValue in
                guard let format else { return ["field": .string(field)] }
                return ["field": .string(field), "format": .string(format)]
            })
    }
}

/// A data file the bundle holds as the source a chart of it reads (PLAN 2.76): the source, and
/// where it is new, the patch that declares it, as `data_attach` declares one.
public struct Attaching: Decodable, Sendable {
    public let path: String
    /// The source's id, less the `@`.
    public let data: String
    public let patch: [JSONValue]
}

extension ScaenaSession {
    /// A clip of `nodes` as `state` shows them (PLAN 2.37, 2.42), as the text the clipboard
    /// holds: each node and what it holds, their overrides, the data they read, and the files
    /// those read, each node's box a share of the canvas.
    public func copying(state: String, nodes: [String]) throws -> String {
        try call("copying", ["state": .string(state), "nodes": .array(nodes.map { .string($0) })])
    }

    /// The patch that pastes `text` in `state` about `point` (canvas units), as Insert places a
    /// node (PLAN 2.37): a clip's nodes under ids new to the deck, or other text as a text in the
    /// theme's body role. The files a clip carries that the bundle lacks are handed over.
    public func pasting(_ text: String, state: String, at point: CGPoint) throws -> Pasted {
        try call(
            "pasting",
            ["text": .string(text), "state": .string(state), "x": .number(Double(point.x)), "y": .number(Double(point.y))])
    }

    /// `node`'s look as `state` shows it (PLAN 2.58): what ⌥⌘C copies, each property its type's
    /// look has, with the value the state shows where the deck sets one.
    public func look(state: String, node: String) throws -> JSONValue {
        try call("look", ["state": .string(state), "node": .string(node)])
    }

    /// `look`, as `look` gives it, put on `nodes` in `state` (PLAN 2.58): what ⌥⌘V makes.
    public func putting(state: String, look: JSONValue, nodes: [String]) throws -> Put {
        try call("putting", ["state": .string(state), "look": look, "nodes": .array(nodes.map { .string($0) })])
    }

    /// `text` pasted as a sheet's cells, read in the deck's language (PLAN 2.96): the source they
    /// would be; none where it is not cells, and pastes as words.
    public func cells(_ text: String) throws -> Cells? {
        try call("cells", ["text": .string(text)])
    }

    /// `path`, a data file the bundle holds, as the source a chart of it reads (PLAN 2.76), its
    /// columns typed by `schema` where given, as a sheet's cells say they read (PLAN 2.96).
    public func attaching(path: String, schema: JSONValue? = nil) throws -> Attaching {
        var args: [String: JSONValue] = ["path": .string(path)]
        if let schema { args["schema"] = schema }
        return try call("attaching", .object(args))
    }

    /// A file dropped or pasted on the canvas (PLAN 2.96), handed over where the bundle keeps it:
    /// a data file (CSV, JSON) under `data/`, anything else, a picture above all, under `assets/`
    /// named by its content. Its path in the bundle.
    public func drop(_ data: Data, named name: String) throws -> String {
        let answer = data.withUnsafeBytes { raw in
            scaena_drop(handle, name, raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count)
        }
        return try Self.decode(answer, as: String.self)
    }
}

/// What another app, or this one, put on the pasteboard, as the canvas pastes it (PLAN 2.37,
/// 2.96): a clip of nodes; a file, a screenshot or a file copied in the Finder, the words beside
/// it only naming it; or words, which a sheet's cells are, and a clip copied as text.
public enum Pasteboard: Equatable, Sendable {
    case clip(String)
    case file(Data, name: String)
    case words(String)

    /// Where a clip of nodes goes on the pasteboard, beside its text.
    public static let clipKind = "com.scaena.clip"

    #if os(macOS)
    /// Where a clip of nodes goes on the pasteboard, beside its text.
    public static let clipType = NSPasteboard.PasteboardType(clipKind)

    /// What `board` holds that the canvas pastes; none where it holds nothing it takes.
    @MainActor
    public static func read(_ board: NSPasteboard = .general) -> Pasteboard? {
        if let clip = board.string(forType: clipType) { return .clip(clip) }
        let words = board.string(forType: .string) ?? ""
        if let found = held(board), namesOnly(words, found.name) { return .file(found.data, name: found.name) }
        return words.isEmpty ? nil : .words(words)
    }

    /// Put `clip`, a clip's text, on `board`: as a clip, and as its text, which the browser reads.
    @MainActor
    public static func write(clip: String, to board: NSPasteboard = .general) {
        board.clearContents()
        board.setString(clip, forType: clipType)
        board.setString(clip, forType: .string)
    }

    /// The file `board` holds: one copied in the Finder, or a picture's data, a PNG or a JPEG as
    /// it is and any other kind of picture as a PNG of it.
    @MainActor
    static func held(_ board: NSPasteboard) -> (data: Data, name: String)? {
        let urls = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]
        if let url = urls?.first, let data = try? Data(contentsOf: url) { return (data, url.lastPathComponent) }
        if let png = board.data(forType: .png) { return (png, "picture.png") }
        if let jpeg = board.data(forType: NSPasteboard.PasteboardType(UTType.jpeg.identifier)) { return (jpeg, "picture.jpg") }
        if let tiff = board.data(forType: .tiff), let png = png(tiff) { return (png, "picture.png") }
        return nil
    }
    #else
    /// What `board` holds that the canvas pastes; none where it holds nothing it takes (PLAN 4.1).
    @MainActor
    public static func read(_ board: UIPasteboard = .general) -> Pasteboard? {
        if let data = board.data(forPasteboardType: clipKind), let clip = String(data: data, encoding: .utf8) {
            return .clip(clip)
        }
        let words = board.string ?? ""
        if let found = held(board), namesOnly(words, found.name) { return .file(found.data, name: found.name) }
        return words.isEmpty ? nil : .words(words)
    }

    /// Put `clip`, a clip's text, on `board`: as a clip, and as its text, which the browser reads.
    @MainActor
    public static func write(clip: String, to board: UIPasteboard = .general) {
        board.items = [[clipKind: Data(clip.utf8), UTType.utf8PlainText.identifier: clip]]
    }

    /// The file `board` holds: one a Files item put there, or a picture's data, a PNG or a JPEG as
    /// it is and any other kind of picture as a PNG of it.
    @MainActor
    static func held(_ board: UIPasteboard) -> (data: Data, name: String)? {
        if let url = board.urls?.first(where: \.isFileURL), let data = try? Data(contentsOf: url) {
            return (data, url.lastPathComponent)
        }
        if let png = board.data(forPasteboardType: UTType.png.identifier) { return (png, "picture.png") }
        if let jpeg = board.data(forPasteboardType: UTType.jpeg.identifier) { return (jpeg, "picture.jpg") }
        for kind in board.types where UTType(kind)?.conforms(to: .image) == true {
            if let data = board.data(forPasteboardType: kind), let png = png(data) { return (png, "picture.png") }
        }
        return nil
    }
    #endif

    /// `data`, a picture of any kind ImageIO reads, as a PNG of it (PLAN 2.96): what an image
    /// shows of a picture of a kind it does not (SPEC §3.3).
    public static func png(_ data: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        let out = NSMutableData()
        guard let made = CGImageDestinationCreateWithData(out as CFMutableData, UTType.png.identifier as CFString, 1, nil)
        else {
            return nil
        }
        CGImageDestinationAddImage(made, image, nil)
        return CGImageDestinationFinalize(made) ? out as Data : nil
    }

    /// Whether the words beside a file only name it (PLAN 2.96): none, its name, or an address
    /// alone, as a picture copied in a browser may carry. A sheet's cells come with a picture of
    /// them, which their words outrank.
    static func namesOnly(_ words: String, _ name: String) -> Bool {
        let w = words.trimmingCharacters(in: .whitespacesAndNewlines)
        return w.isEmpty || w == name || w.range(of: #"^(https?|file)://\S+$"#, options: [.regularExpression, .caseInsensitive]) != nil
    }
}

// MARK: A drop

extension Pasteboard {
    /// What a drop on the canvas takes (PLAN 3.22): a file, from the Finder or Files, or a
    /// picture's data, from Photos or a browser.
    public static let droppable: [UTType] = [.fileURL, .image, .commaSeparatedText, .json]

    /// The file `item`, one thing dropped, holds that the canvas takes, as `held` reads one from
    /// the pasteboard: a CSV or a JSON file, or a PNG or a JPEG, as it is; any other picture as a
    /// PNG of it. Each is named as it was, or by its kind where it had no name of its kind. None
    /// where it holds none of them.
    @MainActor
    public static func dropped(_ item: NSItemProvider) async -> (data: Data, name: String)? {
        let kinds = item.registeredTypeIdentifiers.compactMap { UTType($0) }
        for kind in [UTType.commaSeparatedText, .json, .png, .jpeg] where kinds.contains(where: { $0.conforms(to: kind) }) {
            if let file = await file(item, kind) { return (file.data, named(file.name, as: kind)) }
        }
        if let kind = kinds.first(where: { $0.conforms(to: .image) }), let file = await file(item, kind),
            let png = png(file.data)
        {
            return (png, named((file.name as NSString).deletingPathExtension, as: .png))
        }
        return nil
    }

    /// The bytes and the name of the file `item` gives as `kind`, read while it is there: a file
    /// the Finder or Files dragged, or a copy written of a picture's data.
    @MainActor
    private static func file(_ item: NSItemProvider, _ kind: UTType) async -> (data: Data, name: String)? {
        await withCheckedContinuation { done in
            _ = item.loadFileRepresentation(forTypeIdentifier: kind.identifier) { url, _ in
                let read = url.flatMap { url in (try? Data(contentsOf: url)).map { (data: $0, name: url.lastPathComponent) } }
                done.resume(returning: read)
            }
        }
    }

    /// `name`, with an extension of `kind` where it has none of it: `dropped` where it has no name.
    static func named(_ name: String, as kind: UTType) -> String {
        let ext = (name as NSString).pathExtension
        if !ext.isEmpty, let found = UTType(filenameExtension: ext), found.conforms(to: kind) { return name }
        return "\(name.isEmpty ? "dropped" : name).\(kind.preferredFilenameExtension ?? "bin")"
    }
}
