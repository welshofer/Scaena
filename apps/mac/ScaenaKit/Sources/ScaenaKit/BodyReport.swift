#if DEBUG
import Foundation
import os

/// How often the window's views are evaluated, and which values are set fast, where the app is
/// launched with `-reportAnimations YES`, as the iPad's UI tests launch it (lift-up 20261110):
/// what keeps the toolbar's glass and the segmented controls' lenses animating, so XCTest's wait
/// for the app to settle sometimes runs out. Debug builds only; a release build carries none of it.
@MainActor
public enum BodyReport {
    private static let log = Logger(subsystem: "com.welshofer.Scaena", category: "bodies")
    private static let asked = UserDefaults.standard.bool(forKey: "reportAnimations")
    /// Each name's count since it was last said, and when that was.
    private static var counts: [String: (count: Int, since: Date)] = [:]

    /// `view`'s body evaluated: counted, and at most once a second the count said and `changes`
    /// run, the view's `Self._logChanges()`, which names in SwiftUI's own log
    /// (`com.apple.SwiftUI`, "Changed Body Properties") what changed this time.
    public static func evaluated(_ view: String, changes: () -> Void) {
        guard asked, let (count, over) = counted(view) else { return }
        log.debug("\(view, privacy: .public): \(count) bodies in \(over, format: .fixed(precision: 1)) s")
        changes()
    }

    /// `value` set, by its owner (`"playhead, by ScaenaCanvas"`): counted, and said where it was
    /// set more than ten times in the second since it was last said.
    public static func set(_ value: String) {
        guard asked, let (count, over) = counted(value), Double(count) > 10 * over else { return }
        log.debug("\(value, privacy: .public): set \(count) times in \(over, format: .fixed(precision: 1)) s")
    }

    /// `name` counted: where a second has gone by since it was last said, the count since then,
    /// this one included, and the seconds it was counted over; else none. The first is said at
    /// once. Each count runs on from the last said, so a burst after a quiet spell is counted
    /// whole, never one at a time (lift-up 20261110: the count reset to none, so the next was
    /// always said alone, as 1).
    private static func counted(_ name: String) -> (Int, Double)? {
        let now = Date()
        guard var entry = counts[name] else {
            counts[name] = (count: 0, since: now)
            return (1, 0)
        }
        entry.count += 1
        let over = now.timeIntervalSince(entry.since)
        guard over >= 1 else {
            counts[name] = entry
            return nil
        }
        counts[name] = (count: 0, since: now)
        return (entry.count, over)
    }
}
#endif
