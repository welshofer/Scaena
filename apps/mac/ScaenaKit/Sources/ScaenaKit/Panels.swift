import CScaena
import Foundation
import Synchronization

// The panels (PLAN 3.15), as the browser's tabs: the theme (PLAN 2.39, 2.61, 2.94), the data
// (PLAN 2.55), the bundle's files (PLAN 2.59), and the versions its history keeps (PLAN 2.60).

/// The errors among what a change added, as lint found them: why it was refused. The window says
/// each by `DeckEditor.refusal` (PLAN 3.18), not by lint's own message, which names what it finds by
/// the ids the deck writes.
private func errors(in added: [JSONValue]) -> [JSONValue] {
    added.filter { $0["severity"]?.string == "error" }
}

/// Lint's own message for each of `errors`, as the CLI says it.
private func messages(_ errors: [JSONValue]) -> [String] {
    errors.compactMap { $0["message"]?.string }
}

/// A theme that ships (SPEC §3.6): its name, as `retheme(ships:)` takes it, its family, the
/// section a chooser offers it under (PLAN T.0d), and its file.
public struct ShippedTheme: Decodable, Identifiable, Equatable, Sendable {
    public let name: String
    public let family: String
    public let file: String

    public var id: String { name }
}

/// A family of the themes that ship, as a chooser's section offers it (PLAN T.0d): its name and
/// its themes, in the registry's order.
public struct ThemeFamily: Identifiable, Equatable, Sendable {
    public let name: String
    public let themes: [ShippedTheme]

    public var id: String { name }
}

extension [ShippedTheme] {
    /// These themes by family, each family a section, in the order the list first names it.
    public var byFamily: [ThemeFamily] {
        var order: [String] = []
        var held: [String: [ShippedTheme]] = [:]
        for theme in self {
            if held[theme.family] == nil { order.append(theme.family) }
            held[theme.family, default: []].append(theme)
        }
        return order.map { ThemeFamily(name: $0, themes: held[$0] ?? []) }
    }
}

/// The theme the deck names, and the theme files the bundle holds.
public struct Themes: Decodable, Equatable, Sendable {
    /// Its path, `(inline)` for one written in the deck, or none.
    public let current: String?
    public let files: [String]
}

/// The theme the deck names, as its text: what the theme panel shows (PLAN 2.61).
/// One kind of code token as the theme draws it (PLAN 2.144): the kind as the theme keys its look
/// (`keyword`) and as a person calls it (Keywords), the color it is drawn in (`#rrggbb`), the color
/// the theme names for it (none where the accent derives it), and whether that was moved to read
/// on its ground.
public struct CodeColor: Decodable, Sendable, Identifiable {
    public let kind: String
    public let said: String
    public let drawn: String
    public let named: String?
    public let moved: Bool

    public var id: String { kind }

    /// The edit of `theme` that names `value` the color of code's `kind`, or, with none, lets the
    /// accent derive it: the objects it goes in added where the theme has none, and a kind left
    /// with nothing taken away. None where nothing changes.
    public static func coloring(_ kind: String, _ value: String?, in theme: JSONValue?) -> [JSONValue] {
        let code = theme?["code"]
        let syntax = code?["syntax"]
        let held = syntax?[kind]
        let path = "/code/syntax/\(kind)"
        guard let value else {
            guard let held, held["color"] != nil else { return [] }
            let alone = (held.object?.count ?? 0) <= 1
            return [["op": "remove", "path": .string(alone ? path : "\(path)/color")]]
        }
        let color = JSONValue.string(value)
        guard code != nil else { return [["op": "add", "path": "/code", "value": ["syntax": [kind: ["color": color]]]]] }
        guard syntax != nil else { return [["op": "add", "path": "/code/syntax", "value": [kind: ["color": color]]]] }
        guard let held else { return [["op": "add", "path": .string(path), "value": ["color": color]]] }
        return [["op": .string(held["color"] == nil ? "add" : "replace"), "path": .string("\(path)/color"), "value": color]]
    }
}

/// Two are equal where they read one theme as one text: its JSON, decoded from that text, is not
/// compared, since SwiftUI compares what a view holds each time the view is made again, and a
/// shipped theme's JSON compared value by value, read afresh after each edit, kept the window from
/// answering for seconds (the hang watch's samples of an insert and a drop, #1309).
public struct ThemeText: Decodable, Sendable, Equatable {
    /// Its path in the bundle, or `(inline)`.
    public let theme: String
    public let text: String
    /// The theme's JSON, decoded once as the text is read: the Theme panel reads it for each row it
    /// draws, and a shipped theme's 181 layouts decoded again for each kept the window from
    /// answering after every edit (#1262's sample).
    public let json: JSONValue?

    private enum Keys: String, CodingKey { case theme, text }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        theme = try fields.decode(String.self, forKey: .theme)
        text = try fields.decode(String.self, forKey: .text)
        json = ParsedTheme.shared.json(of: text).json
    }
}

/// The theme last parsed, by its text: the inspector reads the theme on every edit and the layout
/// picker on every draw, and a shipped theme's 181 layouts parsed again each time held the window
/// for seconds (#1309's walk, hangs 35, 37, 28, and 30). A text that reads as the last one did
/// takes its JSON as it was parsed; any other is parsed and kept in its place.
final class ParsedTheme: Sendable {
    static let shared = ParsedTheme()
    private let last = Mutex<(text: String, json: JSONValue?)?>(nil)

    /// `text`'s JSON, and whether it was parsed now rather than kept.
    func json(of text: String) -> (json: JSONValue?, parsed: Bool) {
        if let kept = last.withLock({ $0 }), kept.text == text { return (kept.json, false) }
        let json = try? JSONDecoder().decode(JSONValue.self, from: Data(text.utf8))
        last.withLock { $0 = (text, json) }
        return (json, true)
    }

    public static func == (a: ThemeText, b: ThemeText) -> Bool { a.theme == b.theme && a.text == b.text }
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
    /// The errors it would have made, as lint found them: what the window says it by.
    public let refusals: [JSONValue]
    /// How many texts the theme left too faint to read were set in its color that reads where
    /// each stands (PLAN 2.82).
    public let recolored: Int
    /// How many texts the theme set past their boxes, or over another, were fitted as lint's
    /// checked fix fits them: shrunk to fit.
    public let refitted: Int

    private enum Keys: String, CodingKey { case theme, applied, refused, added, recolored, refitted }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        theme = try fields.decode(String.self, forKey: .theme)
        applied = try fields.decode(Bool.self, forKey: .applied)
        refused = try fields.decodeIfPresent(Bool.self, forKey: .refused) ?? false
        let found = errors(in: try fields.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        refusals = found
        why = messages(found)
        recolored = try fields.decodeIfPresent([JSONValue].self, forKey: .recolored)?.count ?? 0
        refitted = try fields.decodeIfPresent([JSONValue].self, forKey: .refitted)?.count ?? 0
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
    /// The errors it would have made, as lint found them: what the window says it by.
    public let refusals: [JSONValue]
    public let files: [Rewritten]

    private enum Keys: String, CodingKey { case edited, files }
    private enum Edited: String, CodingKey { case theme, applied, refused, added }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let edited = try fields.nestedContainer(keyedBy: Edited.self, forKey: .edited)
        theme = try edited.decode(String.self, forKey: .theme)
        applied = try edited.decode(Bool.self, forKey: .applied)
        refused = try edited.decodeIfPresent(Bool.self, forKey: .refused) ?? false
        let found = errors(in: try edited.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        refusals = found
        why = messages(found)
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
    /// The errors it would have made, as lint found them: what the window says it by.
    public let refusals: [JSONValue]
    /// Whether it wrote: the file, for a source that is one; else the deck's rows.
    public let wrote: Bool
    /// The file it is: none for rows written inline.
    public let file: String?
    /// What the window says of each figure that quoted a row the edit took away, which quotes the
    /// data no more, its words kept (R1's agent dogfood 10): "Slide 7's “275” quotes the data no
    /// more". None where the edit dropped no quote.
    public let unquoted: [String]
    /// What the window says of words that still write an edited cell's old value, which the edit
    /// leaves as written: "Slide 9's notes still say 4,605". None where nothing does.
    public let stale: [String]

    private enum Keys: String, CodingKey { case result, wrote }
    private enum Inner: String, CodingKey { case edited, file, added, unquoted, stale }
    private struct Said: Decodable { let said: String }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let result = try fields.nestedContainer(keyedBy: Inner.self, forKey: .result)
        edited = try result.decode(Bool.self, forKey: .edited)
        file = try result.decodeIfPresent(String.self, forKey: .file)
        unquoted = try (result.decodeIfPresent([Said].self, forKey: .unquoted) ?? []).map(\.said)
        stale = try (result.decodeIfPresent([Said].self, forKey: .stale) ?? []).map(\.said)
        let found = errors(in: try result.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        refusals = found
        why = messages(found)
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
    /// What its change did, in a person's words (PLAN 2.99, `scaena_core::words::change`), as the
    /// browser's Versions says it: An object added, an object moved.
    public let said: String
    /// Who made it, in a person's words: you, the assistant and its model, or outside Scaena.
    public let by: String
}

/// What changed from one version to another, or to the deck now, as `scaena history --diff` says
/// it (PLAN 2.60): each state that changed, by id, and how; the deck's own fields that changed; and
/// the files whose bytes changed.
public struct Compared: Decodable, Sendable {
    /// Each state that changed, by id: `added`, `removed`, or `changed`.
    public let states: [(state: String, how: String)]
    public let deck: [String]
    public let files: [String]
    /// What a beat changed, named by the slides it is read on (editor dogfood 7): each label with
    /// those slides, "Presenter Notes" on "Slide 4", so a beat's notes change is named by its
    /// slide, not as "the deck's settings".
    public let story: [(label: String, slides: [String])]

    private enum Keys: String, CodingKey { case states, deck, files, story }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let changed = try fields.decodeIfPresent(JSONValue.self, forKey: .states)?.object ?? [:]
        states = changed.keys.sorted().map { (state: $0, how: Self.how(changed[$0] ?? .null)) }
        deck = (try fields.decodeIfPresent(JSONValue.self, forKey: .deck)?.object ?? [:]).keys.sorted()
        files = try fields.decodeIfPresent([String].self, forKey: .files) ?? []
        story = (try fields.decodeIfPresent([JSONValue].self, forKey: .story) ?? []).compactMap { entry in
            guard let pair = entry.array, let label = pair.first?.string else { return nil }
            let slides = pair.count > 1 ? (pair[1].array?.compactMap(\.string) ?? []) : []
            return (label: label, slides: slides)
        }
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
    /// The errors it would have made, as lint found them: what the window says it by.
    public let refusals: [JSONValue]
    public let files: [Rewritten]
    /// With a slide asked, the slide put back, by its number and headline in the deck it makes:
    /// "Slide 2: Revenue doubled" (dogfood 16).
    public let slide: String?
    /// With a slide asked, each object it shows as it is now since another slide shows it too, in a
    /// sentence: "“Scaena anew” kept as it is now, since Slide 12 shows it too" (dogfood 16).
    public let kept: [String]

    private enum Keys: String, CodingKey { case restored, files }
    private enum Inner: String, CodingKey { case applied, added, slide, kept }

    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: Keys.self)
        let restored = try fields.nestedContainer(keyedBy: Inner.self, forKey: .restored)
        applied = try restored.decode(Bool.self, forKey: .applied)
        let found = errors(in: try restored.decodeIfPresent([JSONValue].self, forKey: .added) ?? [])
        refusals = found
        why = messages(found)
        files = try fields.decodeIfPresent([Rewritten].self, forKey: .files) ?? []
        slide = try restored.decodeIfPresent(String.self, forKey: .slide)
        kept = try restored.decodeIfPresent([String].self, forKey: .kept) ?? []
    }
}

extension ScaenaSession {
    // MARK: The theme

    /// The themes that ship: Dusk, Daybreak, Ember, and Salon, each with its family.
    public func shippedThemes() throws -> [ShippedTheme] { try call("shippedThemes") }

    /// The theme the deck names, and the theme files the bundle holds.
    public func themes() throws -> Themes { try call("themes") }

    /// The theme the deck names, as its text; none where it names none.
    public func themeText() throws -> ThemeText? { try call("themeText") }

    /// Each kind of code token as the theme draws it (PLAN 2.144): what the Theme panel's Code
    /// shows.
    public func codeColors() throws -> [CodeColor] { try call("codeColors") }

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

    /// Draw frames at rest in the theme the deck names as `ops`, RFC 6902 operations on it, would
    /// edit it, with nothing made, validated, or linted: a color held in its picker, before it is
    /// chosen (lift-up ATTR-5). None draws the theme as it is; so does any edit of the deck.
    public func themePreviewing(_ ops: [JSONValue]?) throws {
        let patch: JSONValue = ops.map { JSONValue.array($0) } ?? .null
        let _: JSONValue = try call("themePreviewing", ["ops": patch])
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

    /// The states of the version `viewVersion` shows, in its order, as its own slide list numbers
    /// them (PLAN 2.99): `Slide 3, step 2`.
    public func versionSlides() throws -> [String] { try call("versionSlides") }

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
    /// change, refused as a patch is. With `slide`, a state of the version, only the slide that
    /// holds it is put back, with the data it reads, the rest of the deck as it is (dogfood 16).
    public func restoreVersion(_ version: String, slide: String? = nil, at date: Date = Date()) throws -> Restored {
        var args: [String: JSONValue] = ["version": .string(version), "at": .string(date.formatted(.iso8601))]
        if let slide { args["slide"] = .string(slide) }
        return try call("restoreVersion", .object(args))
    }
}

/// A number as a panel's field shows and reads it (PLAN 3.28): shown without the trailing zeros
/// or the noise a binary fraction leaves, read from what was typed, and moved by a stepper to the
/// places of its step.
public enum FieldNumber {
    /// `value` as a field shows it: `32`, `1.25`, `-0.02`; nothing where there is none.
    public static func shown(_ value: Double?) -> String {
        guard let value, value.isFinite else { return "" }
        let rounded = (value * 10_000).rounded() / 10_000
        if rounded == rounded.rounded(), abs(rounded) < 1e15 { return String(Int(rounded)) }
        var text = String(format: "%.4f", rounded)
        while text.hasSuffix("0") { text.removeLast() }
        if text.hasSuffix(".") { text.removeLast() }
        return text
    }

    /// What was typed, as a number: the blanks about it ignored, and a comma taken for the decimal
    /// point; none where it is no finite number.
    public static func read(_ typed: String) -> Double? {
        let text = typed.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".")
        guard let number = Double(text), number.isFinite else { return nil }
        return number
    }

    /// `value` moved once by `step`, rounded to the places either is shown to.
    public static func stepped(_ value: Double, by step: Double) -> Double {
        let scale = pow(10, Double(max(places(value), places(step))))
        return ((value + step) * scale).rounded() / scale
    }

    /// The places after the point `value` is shown to.
    private static func places(_ value: Double) -> Int {
        shown(value).split(separator: ".").dropFirst().first?.count ?? 0
    }
}

/// The theme's colors as an inspector's swatches show them (PLAN 3.18): two a person cannot tell
/// apart at a swatch's size are one swatch, as two names that give one color are.
public enum Swatch {
    /// How far apart, of 255, two colors' channels may be and still look alike in a swatch: the
    /// near-blacks a dark theme sets its two backgrounds in, say, `#101014` and `#17171D`.
    public static let alike = 12

    /// Whether the colors `a` and `b`, as the theme writes them (`#rrggbb` or `#rrggbbaa`), look
    /// alike in a swatch: none of their channels, alpha too, more than `alike` apart. Colors
    /// written otherwise (`oklch(…)`) look alike only where they are written alike.
    public static func looksAlike(_ a: String, _ b: String) -> Bool {
        guard let x = channels(a), let y = channels(b) else { return a.lowercased() == b.lowercased() }
        return zip(x, y).allSatisfy { abs($0 - $1) <= alike }
    }

    /// `#rrggbb` or `#rrggbbaa` as its red, green, blue, and alpha, each of 255; none for another
    /// kind.
    private static func channels(_ color: String) -> [Int]? {
        guard color.hasPrefix("#"), color.count == 7 || color.count == 9,
            let n = UInt64(color.dropFirst(), radix: 16)
        else { return nil }
        let rgba = color.count == 9 ? n : n << 8 | 0xFF
        return [24, 16, 8, 0].map { Int((rgba >> UInt64($0)) & 0xFF) }
    }
}
