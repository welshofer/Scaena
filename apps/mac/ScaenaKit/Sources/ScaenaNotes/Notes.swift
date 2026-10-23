import SwiftUI

/// Speaker notes as a presenter reads them (SPEC §2.6, lift-up USE-1): the blocks the engine
/// reads a state's notes as (`scaena_core::notes`), lines, a list, or a table, each word plain,
/// bold, italic, or code, and the one view that shows them, so the Mac's presenter window, the
/// iPad's, a rehearsal, and the remote, which holds no engine and is sent them, show one reading.
/// The deck keeps its notes as written; nothing here changes them.

/// A run of words in one look.
public struct NoteSpan: Codable, Sendable, Hashable {
    public var text: String
    public var bold: Bool
    public var italic: Bool
    public var code: Bool

    public init(_ text: String, bold: Bool = false, italic: Bool = false, code: Bool = false) {
        self.text = text
        self.bold = bold
        self.italic = italic
        self.code = code
    }

    enum CodingKeys: String, CodingKey { case text, bold, italic, code }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        text = try c.decode(String.self, forKey: .text)
        bold = try c.decodeIfPresent(Bool.self, forKey: .bold) ?? false
        italic = try c.decodeIfPresent(Bool.self, forKey: .italic) ?? false
        code = try c.decodeIfPresent(Bool.self, forKey: .code) ?? false
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(text, forKey: .text)
        if bold { try c.encode(true, forKey: .bold) }
        if italic { try c.encode(true, forKey: .italic) }
        if code { try c.encode(true, forKey: .code) }
    }
}

/// An item of a list: how deep it is nested, from 0, and its words.
public struct NoteItem: Codable, Sendable, Hashable {
    public var level: Int
    public var spans: [NoteSpan]

    public init(level: Int = 0, spans: [NoteSpan]) {
        self.level = level
        self.spans = spans
    }
}

/// A block of the notes, as the engine says it: `{ kind: "lines" | "list" | "table", … }`.
public enum NoteBlock: Codable, Sendable, Hashable {
    /// Lines as written, each kept on its own; a heading a line set bold.
    case lines([[NoteSpan]])
    /// A list, bulleted or numbered, each item at its level.
    case list(ordered: Bool, items: [NoteItem])
    /// A table: its rows, the first a row of headings where `header`.
    case table(header: Bool, rows: [[[NoteSpan]]])

    enum CodingKeys: String, CodingKey { case kind, lines, ordered, items, header, rows }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "lines": self = .lines(try c.decode([[NoteSpan]].self, forKey: .lines))
        case "list":
            self = .list(ordered: try c.decode(Bool.self, forKey: .ordered), items: try c.decode([NoteItem].self, forKey: .items))
        case "table":
            self = .table(header: try c.decode(Bool.self, forKey: .header), rows: try c.decode([[[NoteSpan]]].self, forKey: .rows))
        case let kind:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: c, debugDescription: "no such block of notes: \(kind)")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .lines(let lines):
            try c.encode("lines", forKey: .kind)
            try c.encode(lines, forKey: .lines)
        case .list(let ordered, let items):
            try c.encode("list", forKey: .kind)
            try c.encode(ordered, forKey: .ordered)
            try c.encode(items, forKey: .items)
        case .table(let header, let rows):
            try c.encode("table", forKey: .kind)
            try c.encode(header, forKey: .header)
            try c.encode(rows, forKey: .rows)
        }
    }

    /// Plain `text` as one block of its lines: what a remote shows for a presenter that sent it
    /// no blocks.
    public static func of(_ text: String) -> [NoteBlock] {
        text.isEmpty ? [] : [.lines(text.split(separator: "\n", omittingEmptySubsequences: false).map { [NoteSpan(String($0))] })]
    }
}

extension [NoteBlock] {
    /// The blocks as words alone: each line on its own, a list's items marked, and a table's cells
    /// set apart by a middle dot, as the engine's `plain` says them; what VoiceOver reads of the
    /// notes, and what a test compares.
    public var plain: String {
        let words = { (line: [NoteSpan]) in line.map(\.text).joined() }
        var out: [String] = []
        for block in self {
            switch block {
            case .lines(let lines):
                out.append(contentsOf: lines.map(words))
            case .list(let ordered, let items):
                var counts = Array(repeating: 0, count: 8)
                for item in items {
                    let level = Swift.min(Swift.max(item.level, 0), 7)
                    counts[level] += 1
                    for deeper in (level + 1)..<8 { counts[deeper] = 0 }
                    let mark = ordered ? "\(counts[level])." : "•"
                    out.append(String(repeating: "  ", count: level) + "\(mark) " + words(item.spans))
                }
            case .table(_, let rows):
                out.append(contentsOf: rows.map { $0.map(words).joined(separator: " · ") })
            }
        }
        return out.joined(separator: "\n")
    }
}

/// The notes shown as a speaker reads them: lines as written, a list with its marks, a table in
/// its columns, and each word in its look; `empty` where there are none.
public struct NotesText: View {
    public let blocks: [NoteBlock]
    public let empty: String

    public init(_ blocks: [NoteBlock], empty: String = "No notes.") {
        self.blocks = blocks
        self.empty = empty
    }

    /// The blocks the presenter sent, else its `text` as lines.
    public init(text: String, blocks: [NoteBlock], empty: String = "No notes.") {
        self.init(blocks.isEmpty ? NoteBlock.of(text) : blocks, empty: empty)
    }

    public var body: some View {
        if blocks.isEmpty {
            Text(empty)
        } else {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                    switch block {
                    case .lines(let lines):
                        Self.text(lines)
                    case .list(let ordered, let items):
                        list(ordered: ordered, items: items)
                    case .table(let header, let rows):
                        table(header: header, rows: rows)
                    }
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(blocks.plain)
        }
    }

    /// `lines` as one text, each on its own line.
    static func text(_ lines: [[NoteSpan]]) -> Text {
        guard let first = lines.first else { return Text("") }
        return lines.dropFirst().reduce(text(first)) { joined, line in joined + Text("\n") + text(line) }
    }

    /// A line's spans, each in its look.
    static func text(_ line: [NoteSpan]) -> Text {
        line.map { span -> Text in
            var text = Text(span.text)
            if span.bold { text = text.bold() }
            if span.italic { text = text.italic() }
            if span.code { text = text.monospaced() }
            return text
        }
        .reduce(Text(""), +)
    }

    private func list(ordered: Bool, items: [NoteItem]) -> some View {
        var counts = Array(repeating: 0, count: 8)
        let marked: [(mark: String, item: NoteItem)] = items.map { item in
            let level = Swift.min(Swift.max(item.level, 0), 7)
            counts[level] += 1
            for deeper in (level + 1)..<8 { counts[deeper] = 0 }
            return (ordered ? "\(counts[level])." : "•", item)
        }
        return VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(marked.enumerated()), id: \.offset) { _, row in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(row.mark).frame(minWidth: 18, alignment: .trailing)
                    Self.text(row.item.spans).frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(.leading, CGFloat(Swift.min(Swift.max(row.item.level, 0), 7)) * 18)
            }
        }
    }

    private func table(header: Bool, rows: [[[NoteSpan]]]) -> some View {
        Grid(alignment: .topLeading, horizontalSpacing: 14, verticalSpacing: 4) {
            ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                GridRow {
                    ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                        let text = Self.text(cell)
                        (header && i == 0 ? text.bold() : text).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                if header && i == 0 { Divider() }
            }
        }
    }
}
