import Foundation

/// What the Story panel reads its story beside (lift-up 20261110 deferred, as the browser's Story
/// tab): lint's findings, by what tells them apart. A read taken before every state was linted
/// keeps the findings of the state shown, and the fixes and Show its rows offer for them; once
/// the lint of every state lands with findings of its own, the story is read again, and only then.
public enum StoryLint {
    /// `findings` by what tells them apart: each one's code, severity, state, node, and whether its
    /// fix applies, as the browser keys them; a message reworded is no other finding.
    public static func keyed(_ findings: [Finding]) -> String {
        findings.map { "\($0.code) \($0.severity.rawValue) \($0.state ?? "") \($0.node ?? "") \($0.fixable ? 1 : 0)" }
            .joined(separator: "\n")
    }

    /// Whether `linted`, every state's lint just landed, reads the story again: where it finds
    /// otherwise than the findings the story shown was read beside.
    public static func rereads(_ linted: String?, readWith: String?) -> Bool {
        guard let linted else { return false }
        return linted != readWith
    }
}

extension DeckEditor {
    /// What lint found in every state, keyed as `StoryLint.keyed` keys it; none while it has
    /// linted only the state shown.
    public var linted: String? { whole ? StoryLint.keyed(findings) : nil }
}
