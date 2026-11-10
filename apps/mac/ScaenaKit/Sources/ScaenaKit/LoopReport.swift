#if DEBUG
import Foundation
import QuartzCore
import os

/// What keeps the main run loop awake, where the app is launched with `-reportAnimations YES`, as
/// the iPad's UI tests launch it (lift-up 20261110). A UI test's wait for the app to go idle, then
/// for its animations, each runs out after a minute; #990's report found SwiftUI's views quiet
/// through a stall while the glass's matched animations ran on, as they do in runs that pass too.
/// So this says, in the `loop` log, how often the main run loop woke in each second it was busy,
/// what share of the second it was awake, and what each slide's display link did on its ticks.
/// Debug builds only; a release build carries none of it.
@MainActor
public enum LoopReport {
    private static let log = Logger(subsystem: "com.welshofer.Scaena", category: "loop")
    private static let asked = UserDefaults.standard.bool(forKey: "reportAnimations")
    /// Wakes a second past which the loop is said to be busy: a display link at 30 Hz or more.
    private static let busyAt = 30.0

    private static var observer: CFRunLoopObserver?
    private static var wakes = 0
    private static var awake: CFTimeInterval = 0
    private static var woke: CFTimeInterval?
    private static var since = CACurrentMediaTime()
    private static var wasBusy = false
    /// Each display link's ticks since the loop was last said, by what each tick did.
    private static var ticks: [String: [String: Int]] = [:]

    /// The main run loop watched from now on, once; nothing where the app was not asked.
    public static func start() {
        guard asked, observer == nil else { return }
        let activities = CFRunLoopActivity.afterWaiting.rawValue | CFRunLoopActivity.beforeWaiting.rawValue
        let watching = CFRunLoopObserverCreateWithHandler(nil, activities, true, Int.max) { _, activity in
            MainActor.assumeIsolated { seen(activity) }
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), watching, .commonModes)
        observer = watching
        log.debug("watching the main run loop")
    }

    /// A display link's tick, by the link's view (`link`), and what it did (`did`): "painted", "no
    /// drawable", "rested", or "failed".
    public static func ticked(_ link: String, _ did: String) {
        guard asked else { return }
        ticks[link, default: [:]][did, default: 0] += 1
    }

    /// A display link paused or running again, said as it changes.
    public static func link(_ link: String, running: Bool, why: String) {
        guard asked else { return }
        log.debug("link \(link, privacy: .public) \(running ? "runs" : "rests", privacy: .public): \(why, privacy: .public)")
    }

    private static func seen(_ activity: CFRunLoopActivity) {
        let now = CACurrentMediaTime()
        if activity.contains(.afterWaiting) {
            wakes += 1
            woke = now
        } else if let woke {
            awake += now - woke
            self.woke = nil
        }
        let over = now - since
        guard over >= 1 else { return }
        let rate = Double(wakes) / over
        let busy = rate > busyAt
        // Each busy second is said, and the second the loop goes quiet again; a quiet loop says
        // nothing, and wakes no one to say it.
        if busy || wasBusy {
            let links = ticks.sorted { $0.key < $1.key }.map { link, did in
                "\(link): " + did.sorted { $0.key < $1.key }.map { "\($0.value) \($0.key)" }.joined(separator: ", ")
            }
            let share = Int((awake / over * 100).rounded())
            log.debug(
                "main loop woke \(wakes) times in \(over, format: .fixed(precision: 1)) s, awake \(share)%; links: \(links.isEmpty ? "none ticked" : links.joined(separator: "; "), privacy: .public)"
            )
        }
        wasBusy = busy
        wakes = 0
        awake = 0
        since = now
        ticks = [:]
    }
}
#endif
