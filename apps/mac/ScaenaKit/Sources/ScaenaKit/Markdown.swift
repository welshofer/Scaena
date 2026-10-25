import Foundation

/// A block of the assistant's answer, which is markdown by nature (narrative ATTR-4): what the
/// browser's `markdown.ts` splits an answer into, so the window shows the same lists, code, and
/// headings. Each block's words are shown with their inline markdown read: bold, italic, code
/// spans, and links.
public enum MarkdownBlock: Equatable, Sendable {
    case paragraph(String)
    case heading(level: Int, String)
    /// A list, ordered or bulleted, each item at its level (two spaces of indent a level).
    case list(ordered: Bool, items: [Item])
    case code(String, language: String?)

    public struct Item: Equatable, Sendable {
        public let level: Int
        public let text: String

        public init(level: Int, text: String) {
            self.level = level
            self.text = text
        }
    }
}

public enum MarkdownBlocks {
    /// `text` split into its blocks: a fence to its closing line, a heading, a list (nested by its
    /// indent), and runs of lines as paragraphs, blank lines between them.
    public static func blocks(of text: String) -> [MarkdownBlock] {
        var out: [MarkdownBlock] = []
        let lines = text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
            .components(separatedBy: "\n")
        var paragraph: [String] = []
        func flush() {
            if !paragraph.isEmpty { out.append(.paragraph(paragraph.joined(separator: "\n"))) }
            paragraph = []
        }
        var i = 0
        while i < lines.count {
            let line = lines[i]
            if let fence = fence(line) {
                flush()
                var code: [String] = []
                i += 1
                while i < lines.count, !closes(lines[i], fence.mark) {
                    code.append(lines[i])
                    i += 1
                }
                out.append(.code(code.joined(separator: "\n"), language: fence.language))
                i += 1
                continue
            }
            if let heading = heading(line) {
                flush()
                out.append(.heading(level: heading.level, heading.text))
                i += 1
                continue
            }
            if let first = item(line) {
                flush()
                var items: [MarkdownBlock.Item] = []
                while i < lines.count {
                    if let next = item(lines[i]) {
                        items.append(.init(level: next.level, text: next.text))
                    } else if !lines[i].trimmingCharacters(in: .whitespaces).isEmpty,
                        lines[i].first?.isWhitespace == true, let last = items.last
                    {
                        // A line indented under an item carries that item on.
                        items[items.count - 1] = .init(
                            level: last.level, text: last.text + " " + lines[i].trimmingCharacters(in: .whitespaces))
                    } else {
                        break
                    }
                    i += 1
                }
                out.append(.list(ordered: first.ordered, items: items))
                continue
            }
            if line.trimmingCharacters(in: .whitespaces).isEmpty {
                flush()
            } else {
                paragraph.append(line)
            }
            i += 1
        }
        flush()
        return out
    }

    /// A fence's mark and language, where `line` opens one: three or more backticks or tildes.
    private static func fence(_ line: String) -> (mark: String, language: String?)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard let first = trimmed.first, first == "`" || first == "~" else { return nil }
        let mark = String(trimmed.prefix { $0 == first })
        guard mark.count >= 3 else { return nil }
        let rest = trimmed.dropFirst(mark.count).trimmingCharacters(in: .whitespaces)
        guard rest.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "+" || $0 == "-" || $0 == "_" }) else { return nil }
        return (mark, rest.isEmpty ? nil : rest)
    }

    /// Whether `line` closes a fence opened with `mark`: as many of its character, or more, alone.
    private static func closes(_ line: String, _ mark: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard let first = mark.first else { return false }
        return trimmed.count >= mark.count && trimmed.allSatisfy { $0 == first }
    }

    private static func heading(_ line: String) -> (level: Int, text: String)? {
        let marks = line.prefix { $0 == "#" }
        guard !marks.isEmpty, marks.count <= 6 else { return nil }
        let rest = line.dropFirst(marks.count)
        guard rest.first == " " || rest.first == "\t" else { return nil }
        var text = rest.trimmingCharacters(in: .whitespaces)
        while text.last == "#" { text.removeLast() }
        return (marks.count, text.trimmingCharacters(in: .whitespaces))
    }

    /// A list item: its indent as a level, whether its marker is a number, and its words.
    private static func item(_ line: String) -> (level: Int, ordered: Bool, text: String)? {
        let indent = line.prefix { $0 == " " || $0 == "\t" }
        var rest = Substring(line.dropFirst(indent.count))
        let level = indent.replacingOccurrences(of: "\t", with: "  ").count / 2
        var ordered = false
        if let first = rest.first, first == "-" || first == "*" || first == "+" {
            rest = rest.dropFirst()
        } else {
            let digits = rest.prefix { $0.isNumber }
            guard !digits.isEmpty else { return nil }
            rest = rest.dropFirst(digits.count)
            guard rest.first == "." || rest.first == ")" else { return nil }
            rest = rest.dropFirst()
            ordered = true
        }
        guard let space = rest.first, space == " " || space == "\t" else { return nil }
        let text = rest.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return nil }
        return (level, ordered, text)
    }
}

extension MarkdownBlocks {
    /// `text` with each link to a web address saying where it goes: its host after its words,
    /// where they do not name it, as the browser's `webLink` shows it. A deck's words and a tool's
    /// results reach the model, and a link it echoes from them would read as the app's own
    /// (lift-up 20261025 SEC-5).
    public static func hosted(_ text: AttributedString) -> AttributedString {
        var out = AttributedString()
        for (link, range) in text.runs[\.link] {
            out.append(text[range])
            guard let link, let host = host(of: link),
                !String(text[range].characters).lowercased().contains(host)
            else { continue }
            var said = AttributedString(" (\(host))")
            said.link = link
            out.append(said)
        }
        return out
    }

    /// The host `url` goes to, in lower case, without a port or `www.`: none where it is no web
    /// address.
    public static func host(of url: URL) -> String? {
        guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
            var host = url.host()?.lowercased(), !host.isEmpty
        else { return nil }
        if host.hasPrefix("www.") { host.removeFirst(4) }
        return host
    }
}
