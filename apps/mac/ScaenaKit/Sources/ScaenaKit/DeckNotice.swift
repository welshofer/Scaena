import Foundation

/// What the system says of a deck made from a prompt while the person is in another app (lift-up
/// 20261110 ATTR-4): ready, with what was made, or not made, with why. Research, the outline, the
/// slides, and the look run for minutes, and people switch away.
public struct DeckNotice: Equatable, Sendable {
    public let title: String
    public let body: String

    /// The deck about `topic` made, `made` saying what it came to.
    public static func made(about topic: String, _ made: String) -> DeckNotice {
        DeckNotice(title: "Your deck about \(short(topic)) is ready", body: made)
    }

    /// The deck about `topic` not made, `why` saying why in a person's words.
    public static func failed(about topic: String, _ why: String) -> DeckNotice {
        DeckNotice(title: "Your deck about \(short(topic)) was not made", body: why)
    }

    /// A topic short enough for a notification's title: its first line, at most 60 characters,
    /// cut at a word with an ellipsis.
    static func short(_ topic: String) -> String {
        let line = topic.split(whereSeparator: \.isNewline).first.map(String.init) ?? topic
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.count > 60 else { return trimmed.isEmpty ? "your topic" : trimmed }
        let cut = trimmed.prefix(60)
        let word = cut.lastIndex(of: " ").map { cut[..<$0] } ?? cut
        return word.trimmingCharacters(in: .whitespaces) + "…"
    }
}
