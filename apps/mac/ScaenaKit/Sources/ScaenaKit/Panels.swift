import CScaena
import Foundation

// The panels (PLAN 3.15), as the browser's tabs: the theme (PLAN 2.39, 2.61, 2.94), the data
// (PLAN 2.55), the bundle's files (PLAN 2.59), and the versions its history keeps (PLAN 2.60).

/// The errors among what a change added, as lint says them: why it was refused.
private func errors(in added: [JSONValue]) -> [String] {
    added.filter { $0["severity"]?.string == "error" }.compactMap { $0["message"]?.string }
}

/// A theme that ships (SPEC §3.6): its name, as `retheme(ships:)` takes it, and its file.
public struct ShippedTheme: Decodable, Identifiable, Equatable, Sendable {
    public let name: String
    public let file: String

    public var id: String { name }
}

/// The theme the deck names, and the theme files the bundle holds.
public struct Themes: Decodable, Equatable, Sendable {
    /// Its path, `(inline)` for one written in the deck, or none.
    public let current: String?
    public let files: [String]
}

/// The theme the deck names, as its text: what the theme panel shows (PLAN 2.61).
public struct ThemeText: Decodable, Sendable {
    /// Its path in the bundle, or `(inline)`.
    public let theme: String
    public let text: String

    /// The theme's JSON.
    public var json: JSONValue? { try? JSONDecoder().decode(JSONValue.self, from: Data(text.utf8)) }
}

/// What a re-theme did, as `scaena theme --apply` says it (PLAN 2.39).
public struct Themed: Decodable, Sendable {
    /// The theme's path in the bundle.
    public let theme: String
    /// Whether the deck now names it: not when refused.
    public let applied: Bool
    /// Refused: the deck would not validate in it, and keeps its own; `why` says how.
    public let refused: Bool
    public let why: [String]

    private enum Keys: String, CodingKey { case theme, applied, refused, added }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        theme = try fields.decode(String.self, forKey: .theme)
        applied = try fields.decode(Bool.self, forKey: .applied)
        refused = try fields.decodeIfPresent(Bool.self, forKey: .refused) ?? false
        why = errors(in: try fields.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
    }
}

/// What a theme edit did, as `scaena theme --edit` says it (PLAN 2.61, ADR-0016), and the theme
/// file it wrote, before and after, for the undo.
public struct ThemeEdited: Decodable, Sendable {
    /// The theme edited: its path in the bundle, or `(inline)`.
    public let theme: String
    /// Whether the edit stands: not under a dry run, nor when refused.
    public let applied: Bool
    public let refused: Bool
    /// Why it was refused: what it would have made wrong.
    public let why: [String]
    public let files: [Rewritten]

    private enum Keys: String, CodingKey { case edited, files }
    private enum Edited: String, CodingKey { case theme, applied, refused, added }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let edited = try fields.nestedContainer(keyedBy: Edited.self, forKey: .edited)
        theme = try edited.decode(String.self, forKey: .theme)
        applied = try edited.decode(Bool.self, forKey: .applied)
        refused = try edited.decodeIfPresent(Bool.self, forKey: .refused) ?? false
        why = errors(in: try edited.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        files = try fields.decodeIfPresent([Rewritten].self, forKey: .files) ?? []
    }
}

/// One of the bundle's images, fonts, or data files, as `scaena files` lists it (PLAN 2.59).
public struct BundleFile: Decodable, Identifiable, Sendable {
    public let path: String
    /// `image`, `font`, or `data`.
    public let type: String
    public let bytes: Int
    /// What in the deck or its theme names it, as a sentence says each; none, and it may be taken
    /// out.
    public let named: [String]
    /// The nodes drawn from it.
    public let used: [String]

    public var id: String { path }

    private enum Keys: String, CodingKey { case path, type, bytes, named, used }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        path = try fields.decode(String.self, forKey: .path)
        type = try fields.decode(String.self, forKey: .type)
        bytes = try fields.decode(Int.self, forKey: .bytes)
        let named = try fields.decodeIfPresent([JSONValue].self, forKey: .named) ?? []
        self.named = named.map(Self.said)
        let used = try fields.decodeIfPresent([JSONValue].self, forKey: .used) ?? []
        self.used = used.compactMap { $0["node"]?.string }
    }

    /// What names a file, as a sentence says it: `the node cover-photo`, `the data source q3`.
    private static func said(_ named: JSONValue) -> String {
        let field = { (key: String) in named[key]?.string ?? "?" }
        switch named["by"]?.string {
        case "node": return "the node \(field("node"))"
        case "evidence": return "the beat \(field("beat")), as its evidence"
        case "font": return "the deck's font \(field("family"))"
        case "theme": return "the theme's family \(field("family"))"
        case "source": return "the data source \(field("source"))"
        default: return "the deck"
        }
    }
}

/// A data source the deck declares: its name, and the file it is; none for rows written inline.
public struct DataSource: Decodable, Identifiable, Equatable, Sendable {
    public let name: String
    public let file: String?

    public var id: String { name }
}

/// A data source as a sheet, as `scaena data` reads it (PLAN 2.55, SPEC §3.10): its columns and
/// their types, each row's cells as written, and the cells a column does not read, with why.
public struct Sheet: Decodable, Sendable {
    public let columns: [Column]
    public let rows: [[String]]
    public let problems: [Problem]
    /// The file it is: none for rows written inline.
    public let file: String?

    public struct Column: Decodable, Equatable, Sendable {
        public let name: String
        /// `string`, `number`, `date`, or `boolean`: what a value set in it must read as.
        public let type: String
    }

    public struct Problem: Decodable, Equatable, Sendable {
        public let row: Int
        public let column: String
        public let why: String
    }

    private enum Keys: String, CodingKey { case sheet, file }
    private enum Inner: String, CodingKey { case columns, rows, problems }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let sheet = try fields.nestedContainer(keyedBy: Inner.self, forKey: .sheet)
        columns = try sheet.decode([Column].self, forKey: .columns)
        rows = try sheet.decode([[String]].self, forKey: .rows)
        problems = try sheet.decodeIfPresent([Problem].self, forKey: .problems) ?? []
        file = try fields.decodeIfPresent(String.self, forKey: .file)
    }

    /// Why the cell at `row` and `column` does not read, where it does not.
    public func problem(row: Int, column: String) -> String? {
        problems.first { $0.row == row && $0.column == column }?.why
    }
}

/// An edit of a data source's rows (PLAN 2.55), as `data_edit` takes it. Row 0 is the first after
/// the header.
public enum RowEdit: Sendable, Equatable {
    /// `column` of `row` set to `value` as typed: empty is nothing.
    case set(row: Int, column: String, value: String)
    /// A row added at `row`, or at the end, each column its value in `values`.
    case add(row: Int?, values: [String: String])
    case remove(row: Int)

    var json: JSONValue {
        switch self {
        case .set(let row, let column, let value):
            return ["op": "set", "row": .number(Double(row)), "column": .string(column), "value": .string(value)]
        case .add(let row, let values):
            var op: [String: JSONValue] = ["op": "add", "values": .object(values.mapValues { JSONValue.string($0) })]
            if let row { op["row"] = .number(Double(row)) }
            return .object(op)
        case .remove(let row):
            return ["op": "remove", "row": .number(Double(row))]
        }
    }
}

/// What a data edit did (PLAN 2.55): whether it was written, and why not; and whether it wrote the
/// source's file, which the session's data undo takes back, or the deck, which the source's does.
public struct DataEdited: Decodable, Sendable {
    public let edited: Bool
    public let why: [String]
    /// Whether it wrote: the file, for a source that is one; else the deck's rows.
    public let wrote: Bool
    /// The file it is: none for rows written inline.
    public let file: String?

    private enum Keys: String, CodingKey { case result, wrote }
    private enum Inner: String, CodingKey { case edited, file, added }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let result = try fields.nestedContainer(keyedBy: Inner.self, forKey: .result)
        edited = try result.decode(Bool.self, forKey: .edited)
        file = try result.decodeIfPresent(String.self, forKey: .file)
        why = errors(in: try result.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        wrote = try fields.decode(Bool.self, forKey: .wrote)
    }
}

/// A version of the deck its history keeps (PLAN 2.60): as it was just after one change.
public struct Version: Decodable, Identifiable, Equatable, Sendable {
    /// Its place in the history, oldest first, from 1.
    public let n: Int
    /// What names it for as long as the history lasts: its change's id.
    public let id: String
    /// Who made the change: `user`, `agent:<name>`, or `fs`.
    public let author: String?
    public let message: String?
    /// When, in RFC 3339, UTC.
    public let at: String?
    /// How many operations it holds: a character typed is one.
    public let ops: Int
}

/// What changed from one version to another, or to the deck now, as `scaena history --diff` says
/// it (PLAN 2.60): each state that changed, by id, and how; the deck's own fields that changed; and
/// the files whose bytes changed.
public struct Compared: Decodable, Sendable {
    /// Each state that changed, by id: `added`, `removed`, or `changed`.
    public let states: [(state: String, how: String)]
    public let deck: [String]
    public let files: [String]

    private enum Keys: String, CodingKey { case states, deck, files }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let changed = try fields.decodeIfPresent(JSONValue.self, forKey: .states)?.object ?? [:]
        states = changed.keys.sorted().map { (state: $0, how: Self.how(changed[$0] ?? .null)) }
        deck = (try fields.decodeIfPresent(JSONValue.self, forKey: .deck)?.object ?? [:]).keys.sorted()
        files = try fields.decodeIfPresent([String].self, forKey: .files) ?? []
    }

    /// How a state changed, as `StateChange` serializes it: `{"added": true}`, `{"removed": true}`,
    /// or `{"changed": {…}}`.
    private static func how(_ change: JSONValue) -> String {
        change.object?.keys.first ?? "changed"
    }
}

/// A version made the deck again (PLAN 2.60): whether it was, why not, and each file it wrote,
/// before and after, for the undo.
public struct Restored: Decodable, Sendable {
    public let applied: Bool
    public let why: [String]
    public let files: [Rewritten]

    private enum Keys: String, CodingKey { case restored, files }
    private enum Inner: String, CodingKey { case applied, added }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let restored = try fields.nestedContainer(keyedBy: Inner.self, forKey: .restored)
        applied = try restored.decode(Bool.self, forKey: .applied)
        why = errors(in: try restored.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        files = try fields.decodeIfPresent([Rewritten].self, forKey: .files) ?? []
    }
}

extension ScaenaSession {
    // MARK: The theme

    /// The themes that ship: Dusk, Daybreak, and Ember.
    public func shippedThemes() throws -> [ShippedTheme] { try call("shippedThemes") }

    /// The theme the deck names, and the theme files the bundle holds.
    public func themes() throws -> Themes { try call("themes") }

    /// The theme the deck names, as its text; none where it names none.
    public func themeText() throws -> ThemeText? { try call("themeText") }

    /// The deck put in the theme the bundle holds at `path` (PLAN 2.39), by the user: refused, the
    /// deck keeping its own, where it would not validate in it.
    public func retheme(path: String, at date: Date = Date()) throws -> Themed {
        try call("retheme", ["path": .string(path), "at": .string(date.formatted(.iso8601))])
    }

    /// The deck put in the theme that ships as `name`, written into the bundle with its fonts.
    public func retheme(ships name: String, at date: Date = Date()) throws -> Themed {
        try call("retheme", ["ships": .string(name), "at": .string(date.formatted(.iso8601))])
    }

    /// The theme the deck names edited by `ops`, RFC 6902 operations on its JSON (PLAN 2.61), by
    /// the user; with `dryRun`, said and not made.
    public func themeEdit(_ ops: [JSONValue], dryRun: Bool = false, at date: Date = Date()) throws -> ThemeEdited {
        try call(
            "themeEdit", ["ops": .array(ops), "dryRun": .bool(dryRun), "at": .string(date.formatted(.iso8601))])
    }

    /// The theme given the colors of `photo`, an image the bundle holds (PLAN 2.94).
    public func themeEdit(photo: String, at date: Date = Date()) throws -> ThemeEdited {
        try call("themeEdit", ["photo": .string(photo), "at": .string(date.formatted(.iso8601))])
    }

    // MARK: The files

    /// The bundle's images, fonts, and data files (PLAN 2.59).
    public func bundleFiles() throws -> [BundleFile] { try call("bundleFiles") }

    /// `path`, a file nothing names, taken out (PLAN 2.59): `dataUndo` puts it back.
    public func removeFile(_ path: String) throws {
        let _: JSONValue = try call("removeFile", ["path": .string(path)])
    }

    // MARK: The data

    /// The deck's data sources, in its order.
    public func dataSources() throws -> [DataSource] { try call("dataSources") }

    /// Source `name` as a sheet.
    public func dataSheet(_ name: String) throws -> Sheet { try call("dataSheet", ["name": .string(name)]) }

    /// `edits` made in source `name`, all or none, by the user (PLAN 2.55).
    @discardableResult
    public func dataEdit(_ name: String, _ edits: [RowEdit], at date: Date = Date()) throws -> DataEdited {
        try call(
            "dataEdit",
            ["source": .string(name), "edits": .array(edits.map(\.json)), "at": .string(date.formatted(.iso8601))])
    }

    /// The file the last data edit wrote, or a removal took out, put back; with `redo`, written,
    /// or taken out, again. The source it is, or its path; none where there was nothing to undo.
    public func dataUndo(redo: Bool = false, at date: Date = Date()) throws -> String? {
        try call("dataUndo", ["redo": .bool(redo), "at": .string(date.formatted(.iso8601))])
    }

    // MARK: The versions

    /// Begin a history with the next save, where the bundle keeps none (PLAN 2.87).
    public func keepHistory() throws {
        let _: JSONValue = try call("keepHistory")
    }

    /// Whether the bundle keeps a history, or the next save begins one.
    public func keepsHistory() throws -> Bool { try call("keepsHistory") }

    /// The versions the bundle's history keeps, oldest first (PLAN 2.60); an error where it keeps
    /// none.
    public func versions() throws -> [Version] { try call("versions") }

    /// Version `version` (its number, or its id) shown read only, in a session of its own: its
    /// states' ids. `drawing(version:state:width:)` draws one.
    public func viewVersion(_ version: String) throws -> [String] {
        try call("viewVersion", ["version": .string(version)])
    }

    /// `state` of the version `viewVersion` shows, at rest, as a PNG `width` pixels wide.
    public func versionPNG(_ state: String, width: Int) throws -> Data {
        let asked: JSONValue = ["state": .string(state), "width": .number(Double(width))]
        let args = String(decoding: try JSONEncoder().encode(asked), as: UTF8.self)
        var error: UnsafeMutablePointer<CChar>?
        return try Self.take(scaena_export(handle, "version", args, &error), error)
    }

    /// What changed from version `from` to version `to`, or to the deck now.
    public func compareVersions(from: String, to: String? = nil) throws -> Compared {
        var args: [String: JSONValue] = ["from": .string(from)]
        if let to { args["to"] = .string(to) }
        return try call("compareVersions", .object(args))
    }

    /// Version `version` made the deck again, with its data files and its theme, by the user: one
    /// change, refused as a patch is.
    public func restoreVersion(_ version: String, at date: Date = Date()) throws -> Restored {
        try call("restoreVersion", ["version": .string(version), "at": .string(date.formatted(.iso8601))])
    }
}
