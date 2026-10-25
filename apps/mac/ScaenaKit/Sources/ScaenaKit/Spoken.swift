import SwiftUI

/// What VoiceOver is told as it happens, where a person who cannot see the window would otherwise
/// hear nothing (lift-up 10-25c USE-1), as the browser's live regions tell a screen reader: each
/// step of a deck being made, an export made, what the canvas says of an edit, and the assistant's
/// answer done or failed. A failure is said at once, ahead of what VoiceOver is saying.
public enum Spoken {
    /// Say `words`; `urgent`, ahead of what VoiceOver is saying.
    @MainActor public static func say(_ words: String, urgent: Bool = false) {
        guard !words.isEmpty else { return }
        var said = AttributedString(words)
        said.accessibilitySpeechAnnouncementPriority = urgent ? .high : .default
        AccessibilityNotification.Announcement(said).post()
    }
}
