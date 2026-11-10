import ScaenaKit
import UserNotifications
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A deck made from a prompt, or not made, said while the person is in another app (lift-up
/// 20261110 ATTR-4): a local notification, and on the Mac the Dock icon bounced once. Nothing is
/// said while the app is the one in front, where the window says it.
@MainActor
enum Notifying {
    static func tell(_ notice: DeckNotice) {
        #if os(macOS)
        guard !NSApp.isActive else { return }
        NSApp.requestUserAttention(.informationalRequest)
        #else
        guard UIApplication.shared.applicationState != .active else { return }
        #endif
        // A process with no bundle (a test run) has no notification center to ask.
        guard Bundle.main.bundleIdentifier != nil else { return }
        let content = UNMutableNotificationContent()
        content.title = notice.title
        content.body = notice.body
        content.sound = .default
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        let center = UNUserNotificationCenter.current()
        Task {
            guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }
            try? await center.add(request)
        }
    }
}
