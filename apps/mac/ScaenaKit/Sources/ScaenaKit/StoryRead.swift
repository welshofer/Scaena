import Foundation

/// The deck's story as the Story panel shows it (lift-up 20261110 deferred, as the browser's Story
/// tab keeps it): read from the deck as it stands, or, while the source does not compile, the last
/// read kept, said to be as last read, its fixes held, since each was made against a deck the
/// source no longer is.
public struct StoryRead: Sendable {
    public let health: Health
    /// Whether this is the last read, kept while the source has an error to fix.
    public let asLastRead: Bool

    public init(health: Health, asLastRead: Bool) {
        self.health = health
        self.asLastRead = asLastRead
    }

    /// What the panel says over a story kept as last read, in the browser's words.
    public static let said = "Showing the story as last read: the source has an error to fix"

    /// `label`, the findings' list, as a reader hears it: ", as last read" after it where it is.
    public func listed(_ label: String) -> String {
        asLastRead ? "\(label), as last read" : label
    }
}

extension DeckEditor {
    /// Whether the source compiles to a deck that validates, which a story is read of.
    public var compiles: Bool { valid && error == nil }

    /// The story to show after `last`: while the source does not compile, `last` kept as last
    /// read, nothing read of a deck the source no longer is; else the deck's story read now, away
    /// from the window as `health` reads it, none where an edit dropped the read.
    public func story(after last: Health?, isolation: isolated (any Actor)? = #isolation) async throws -> StoryRead? {
        if !compiles, let last { return StoryRead(health: last, asLastRead: true) }
        guard let read = try await health() else { return nil }
        return StoryRead(health: read, asLastRead: false)
    }
}
