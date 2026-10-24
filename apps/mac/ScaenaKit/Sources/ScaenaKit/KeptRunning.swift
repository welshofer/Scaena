#if !os(macOS)
import UIKit

/// Work iPadOS lets run on in the background for as long as it allows (lift-up REL-5): a deck being
/// made takes minutes, and a person who goes to another app meanwhile would otherwise have the
/// run stopped with no word of why. `expired` runs when iPadOS will give no more time, before the
/// time is handed back; `end` hands it back once the work is done, once.
@MainActor
public final class KeptRunning {
    /// What asks iPadOS for the time, and what hands it back: UIApplication's, or a test's.
    public struct System {
        public var begin: @MainActor (String, @escaping @MainActor @Sendable () -> Void) -> UIBackgroundTaskIdentifier
        public var end: @MainActor (UIBackgroundTaskIdentifier) -> Void

        public init(
            begin: @escaping @MainActor (String, @escaping @MainActor @Sendable () -> Void) -> UIBackgroundTaskIdentifier,
            end: @escaping @MainActor (UIBackgroundTaskIdentifier) -> Void
        ) {
            self.begin = begin
            self.end = end
        }

        /// UIApplication's.
        public static var app: System {
            System(
                begin: { name, expired in
                    UIApplication.shared.beginBackgroundTask(withName: name) { MainActor.assumeIsolated { expired() } }
                },
                end: { UIApplication.shared.endBackgroundTask($0) })
        }
    }

    /// What a person reads when iPadOS stopped the run.
    public static let stopped =
        "The deck wasn't finished: iPadOS stopped Scaena in the background. Keep it in front while a deck is made."

    private let system: System
    private let expired: @MainActor () -> Void
    private var held = UIBackgroundTaskIdentifier.invalid

    public init(_ name: String, system: System = .app, expired: @escaping @MainActor () -> Void) {
        self.system = system
        self.expired = expired
        held = system.begin(name) { [weak self] in self?.expire() }
    }

    /// Whether the time is still held.
    public var holding: Bool { held != .invalid }

    /// Hands the time back; once, however often it is called.
    public func end() {
        guard held != .invalid else { return }
        system.end(held)
        held = .invalid
    }

    private func expire() {
        guard holding else { return }
        expired()
        end()
    }
}
#endif
