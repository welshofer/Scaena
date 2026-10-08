import CoreGraphics
import Foundation

/// Where a caret stands in a text, as the engine laid it out (ADR-0013, PLAN 2.32): the text as
/// written, and its lines, each with its characters' edges. Offsets are UTF-16 code units, as
/// `NSString` and the input system count them. Nothing here lays out: the caret, the selection,
/// and where a caret goes up or down a line are read from these, as the browser's canvas reads
/// them (`web/src/typing.ts`).
public struct Carets: Decodable, Equatable, Sendable {
    /// The text as written: the node's `text`, or its runs' texts end to end.
    public let text: String
    public let lines: [Line]
    /// Its paragraphs as a list's items (ADR-0018), by paragraph; none past the last.
    public let items: [ListMark?]?

    /// A line as the engine set it.
    public struct Line: Decodable, Equatable, Sendable {
        /// Its line box's top and bottom, canvas units.
        public let top: Double
        public let bottom: Double
        /// Where a caret on it stands when it holds no character.
        public let x: Double
        /// Where it starts in the text, and where the next line starts.
        public let start: Int
        public let end: Int
        /// It ends at a line break the text sets: a caret after it stands on the next line.
        public let broken: Bool
        /// Its characters in the text's order.
        public let chars: [Char]
    }

    /// A character on a line: where it is in the text, where a caret before it stands, and where
    /// one after it does, the right edge first in right-to-left text.
    public struct Char: Decodable, Equatable, Sendable {
        public let offset: Int
        public let lead: Double
        public let trail: Double

        public init(from decoder: any Decoder) throws {
            var row = try decoder.unkeyedContainer()
            offset = try row.decode(Int.self)
            lead = try row.decode(Double.self)
            trail = try row.decode(Double.self)
        }
    }

    /// A paragraph as a list's item (ADR-0018): its kind, its level, 0 the outermost, and its marker.
    public struct ListMark: Decodable, Equatable, Sendable {
        public let kind: String
        public let level: Int
        public let marker: String
    }

    /// How long the text is, UTF-16.
    public var length: Int { text.utf16.count }

    /// Whether a caret at `offset` can stand on `line`: not past the line break it ends with.
    static func holds(_ line: Line, _ offset: Int) -> Bool {
        line.start <= offset && (offset < line.end || (offset == line.end && !line.broken))
    }

    /// The line a caret at `offset` stands on: `on`, where it can stand there, else the last line
    /// that holds it, so a caret where a line wraps starts the next one.
    public func line(of offset: Int, on: Int? = nil) -> Int {
        if let on, lines.indices.contains(on), Self.holds(lines[on], offset) { return on }
        if let l = lines.indices.last(where: { Self.holds(lines[$0], offset) }) { return l }
        return offset == 0 ? 0 : max(lines.count - 1, 0)
    }

    /// Where a caret at `offset` stands: its line, and its x. Inside a character, before it.
    public func caret(at offset: Int, on: Int? = nil) -> (line: Int, x: Double) {
        let l = line(of: offset, on: on)
        guard lines.indices.contains(l) else { return (0, 0) }
        let line = lines[l]
        let chars = line.chars
        var i = 0
        while i < chars.count && chars[i].offset <= offset { i += 1 }
        guard i > 0 else { return (l, chars.first?.lead ?? line.x) }
        let c = chars[i - 1]
        return (l, c.offset < offset && i == chars.count && offset >= line.end ? c.trail : c.lead)
    }

    /// The offset on line `l` whose caret stands nearest canvas `x`: before one of its characters,
    /// or after the last, the first in the text's order where two are as near.
    public func offset(onLine l: Int, nearest x: Double) -> Int {
        guard lines.indices.contains(l) else { return length }
        let line = lines[l]
        var best = (distance: Double.infinity, offset: line.start)
        for c in line.chars where abs(c.lead - x) < best.distance {
            best = (abs(c.lead - x), c.offset)
        }
        if let last = line.chars.last, Self.holds(line, line.end), abs(last.trail - x) < best.distance {
            best = (abs(last.trail - x), line.end)
        }
        return best.offset
    }

    /// The caret nearest `point`, canvas units as the text is laid out: its offset, and the line it
    /// stands on.
    public func caret(near point: CGPoint) -> (offset: Int, line: Int) {
        let l = lines.firstIndex { Double(point.y) < $0.bottom } ?? max(lines.count - 1, 0)
        return (offset(onLine: l, nearest: Double(point.x)), l)
    }

    /// Where line `l` ends for a caret: before the line break it ends with, if it does.
    public func end(ofLine l: Int) -> Int {
        guard lines.indices.contains(l) else { return length }
        let line = lines[l]
        if line.broken, let last = line.chars.last { return last.offset }
        return line.end
    }

    /// What a selection from `from` to `to` covers: a rectangle per stretch of adjacent characters
    /// on a line, canvas units as the text is laid out.
    public func covered(from: Int, to: Int) -> [CGRect] {
        var out: [CGRect] = []
        for line in lines {
            let spans = line.chars
                .filter { from <= $0.offset && $0.offset < to && $0.lead != $0.trail }
                .map { (min($0.lead, $0.trail), max($0.lead, $0.trail)) }
                .sorted { $0.0 < $1.0 }
            var merged: [(Double, Double)] = []
            for (l, r) in spans {
                if let last = merged.last, l <= last.1 + 0.5 {
                    merged[merged.count - 1].1 = max(last.1, r)
                } else {
                    merged.append((l, r))
                }
            }
            for (l, r) in merged {
                out.append(CGRect(x: l, y: line.top, width: r - l, height: line.bottom - line.top))
            }
        }
        return out
    }

    /// The caret at `offset` as a line box, canvas units as the text is laid out: where an input
    /// method's window goes.
    public func box(at offset: Int, on: Int? = nil) -> CGRect? {
        guard !lines.isEmpty else { return nil }
        let (l, x) = caret(at: offset, on: on)
        let line = lines[l]
        return CGRect(x: x, y: line.top, width: 0, height: line.bottom - line.top)
    }

    // The text's own boundaries, UTF-16: characters as a reader sees them, words, and paragraphs.

    /// Where each character the reader sees (a grapheme cluster) begins, and the text's end.
    private var boundaries: [Int] {
        var out = [0]
        var at = 0
        for c in text {
            at += c.utf16.count
            out.append(at)
        }
        return out
    }

    /// Where the character before `offset` begins.
    public func before(_ offset: Int) -> Int { boundaries.last { $0 < offset } ?? 0 }

    /// Where the character after `offset` ends.
    public func after(_ offset: Int) -> Int { boundaries.first { $0 > offset } ?? length }

    /// The words of the text, as the system finds them.
    private var words: [NSRange] {
        let ns = text as NSString
        var out: [NSRange] = []
        ns.enumerateSubstrings(
            in: NSRange(location: 0, length: ns.length), options: [.byWords, .substringNotRequired]
        ) { _, range, _, _ in
            out.append(range)
        }
        return out
    }

    /// Where the word before `offset` begins.
    public func wordBefore(_ offset: Int) -> Int { words.last { $0.location < offset }?.location ?? 0 }

    /// Where the word after `offset` ends.
    public func wordAfter(_ offset: Int) -> Int {
        words.first { NSMaxRange($0) > offset }.map { NSMaxRange($0) } ?? length
    }

    /// The word at `offset`, or where it stands between two, nothing.
    public func word(at offset: Int) -> NSRange {
        words.first { $0.location <= offset && offset <= NSMaxRange($0) } ?? NSRange(location: offset, length: 0)
    }

    /// The text's paragraphs, each without the break that ends it, as the deck counts them
    /// (`scaena_core::lists::paragraphs`): `\n`, `\r`, `\r\n`, U+2028, and U+2029 end one.
    public var paragraphs: [NSRange] {
        let units = Array(text.utf16)
        var out: [NSRange] = []
        var start = 0
        var i = 0
        while i < units.count {
            let c = units[i]
            if c == 0x0A || c == 0x0D || c == 0x2028 || c == 0x2029 {
                out.append(NSRange(location: start, length: i - start))
                if c == 0x0D, i + 1 < units.count, units[i + 1] == 0x0A { i += 1 }
                start = i + 1
            }
            i += 1
        }
        out.append(NSRange(location: start, length: units.count - start))
        return out
    }

    /// The paragraph `offset` is in, without the break that ends it.
    public func paragraph(at offset: Int) -> NSRange {
        let all = paragraphs
        return all.first { offset <= NSMaxRange($0) } ?? all[all.count - 1]
    }

    /// The paragraphs a selection from `from` to `to` touches: the first's index and the last's.
    public func touched(from: Int, to: Int) -> (first: Int, last: Int) {
        let all = paragraphs
        let at = { (o: Int) in all.firstIndex { o <= NSMaxRange($0) } ?? 0 }
        let first = at(from)
        return (first, max(first, at(to)))
    }

    /// The paragraph at `index` as a list's item, if it is one.
    public func item(_ index: Int) -> ListMark? {
        guard let items, items.indices.contains(index) else { return nil }
        return items[index]
    }

    /// The characters (Unicode scalar values) in the first `units` UTF-16 units: how `replace_text`
    /// and `style_text` count.
    public func scalars(_ units: Int) -> Int {
        var n = 0
        var at = 0
        for s in text.unicodeScalars {
            if at >= units { break }
            at += UTF16.width(s)
            n += 1
        }
        return n
    }
}

/// `point` through `map`, `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`): where a point of a text as
/// laid out is drawn.
func through(_ map: [Double]?, _ point: CGPoint) -> CGPoint {
    guard let m = map, m.count == 6 else { return point }
    let (x, y) = (Double(point.x), Double(point.y))
    return CGPoint(x: m[0] * x + m[2] * y + m[4], y: m[1] * x + m[3] * y + m[5])
}

/// `point`, a point on the canvas, read back through `map`: where it is in the text as laid out.
func back(_ map: [Double]?, _ point: CGPoint) -> CGPoint {
    guard let m = map, m.count == 6 else { return point }
    let det = m[0] * m[3] - m[1] * m[2]
    guard det.isFinite, abs(det) > 1e-12 else { return point }
    let (x, y) = (Double(point.x) - m[4], Double(point.y) - m[5])
    return CGPoint(x: (m[3] * x - m[2] * y) / det, y: (m[0] * y - m[1] * x) / det)
}
