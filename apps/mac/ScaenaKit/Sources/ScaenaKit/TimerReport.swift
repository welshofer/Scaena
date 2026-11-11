#if DEBUG && os(iOS)
import Foundation
import ObjectiveC
import QuartzCore
import os

private let timersLog = Logger(subsystem: "com.welshofer.Scaena", category: "timers")

/// Who keeps the main run loop awake, where the app is launched with `-reportAnimations YES`, as
/// the iPad's UI tests launch it (lift-up 20261110). #996's report found the loop busy in bursts
/// with no slide's display link ticking, the gesture gate timing out within two seconds of a busy
/// second 19 times in 21, and the run loop's timer and source passes equal (every wake runs both,
/// so they named nothing). So this names what is scheduled on it: each `Timer` added to the main run
/// loop, with its interval, whether it repeats, and the code that added it; and each display link
/// made for anything but a slide, by its target's class and selector, counted where it runs when
/// the loop says a busy second. A timer UIKit makes with Core Foundation alone is not seen here.
/// Debug builds only; a release build carries none of it.
public enum TimerReport {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var started = false
    /// Each display link made for something other than a slide, and what it was made for.
    nonisolated(unsafe) private static let links = NSMapTable<CADisplayLink, NSString>.weakToStrongObjects()
    /// When each place that adds timers was last said, and how many it added since.
    nonisolated(unsafe) private static var said: [String: (at: Date, since: Int)] = [:]

    /// The swizzles made, once; nothing where the app was not asked.
    public static func start() {
        guard UserDefaults.standard.bool(forKey: "reportAnimations") else { return }
        lock.lock()
        defer { lock.unlock() }
        guard !started else { return }
        started = true
        swapClass(
            CADisplayLink.self, NSSelectorFromString("displayLinkWithTarget:selector:"),
            #selector(CADisplayLink.scaena_displayLink(withTarget:selector:)))
        swapInstance(
            RunLoop.self, NSSelectorFromString("addTimer:forMode:"), #selector(RunLoop.scaena_add(_:forMode:)))
        swapClass(
            Timer.self, NSSelectorFromString("scheduledTimerWithTimeInterval:repeats:block:"),
            #selector(Timer.scaena_scheduledTimer(withTimeInterval:repeats:block:)))
        swapClass(
            Timer.self, NSSelectorFromString("scheduledTimerWithTimeInterval:target:selector:userInfo:repeats:"),
            #selector(Timer.scaena_scheduledTimer(timeInterval:target:selector:userInfo:repeats:)))
        timersLog.debug("watching the timers and display links made")
    }

    /// The display links made for something other than a slide that run now, by what each was made for.
    static func running() -> [String] {
        lock.lock()
        defer { lock.unlock() }
        var found: [String] = []
        for case let link as CADisplayLink in links.keyEnumerator().allObjects where !link.isPaused {
            let rate = link.preferredFrameRateRange.preferred ?? 0
            found.append("\(links.object(forKey: link) ?? "?") at \(Int(rate)) Hz")
        }
        return found.sorted()
    }

    fileprivate static func made(_ link: CADisplayLink, for target: Any, _ selector: Selector) {
        guard !(target is ScaenaView) else { return }
        let what = "\(type(of: target)).\(NSStringFromSelector(selector))"
        lock.lock()
        links.setObject(what as NSString, forKey: link)
        lock.unlock()
        timersLog.debug("display link made for \(what, privacy: .public), by \(caller(), privacy: .public)")
    }

    /// A timer scheduled on the main run loop: said once for each place that adds it, then at most
    /// once in ten seconds with how many it added since.
    fileprivate static func scheduled(_ interval: TimeInterval, repeats: Bool) {
        guard Thread.isMainThread else { return }
        let by = caller()
        lock.lock()
        let now = Date()
        var entry = said[by] ?? (at: .distantPast, since: 0)
        entry.since += 1
        let say = now.timeIntervalSince(entry.at) >= 10
        if say { said[by] = (at: now, since: 0) } else { said[by] = entry }
        lock.unlock()
        guard say else { return }
        timersLog.debug(
            "timer every \(interval, format: .fixed(precision: 3)) s\(repeats ? ", repeating" : "", privacy: .public), \(entry.since) added since last said, by \(by, privacy: .public)"
        )
    }

    /// The four frames of the stack past the swizzles and this report: who made the call.
    private static func caller() -> String {
        Thread.callStackSymbols.dropFirst(4).prefix(4).map { frame in
            // "3   UIKitCore   0x… -[_UIFoo bar] + 12": the symbol and its offset alone.
            let parts = frame.split(separator: " ", omittingEmptySubsequences: true)
            return parts.count > 3 ? parts[3...].joined(separator: " ") : frame
        }.joined(separator: " < ")
    }

    private static func swapClass(_ type: AnyClass, _ original: Selector, _ replacement: Selector) {
        guard let a = class_getClassMethod(type, original), let b = class_getClassMethod(type, replacement) else {
            timersLog.debug("no \(NSStringFromSelector(original), privacy: .public) to watch")
            return
        }
        method_exchangeImplementations(a, b)
    }

    private static func swapInstance(_ type: AnyClass, _ original: Selector, _ replacement: Selector) {
        guard let a = class_getInstanceMethod(type, original), let b = class_getInstanceMethod(type, replacement) else {
            timersLog.debug("no \(NSStringFromSelector(original), privacy: .public) to watch")
            return
        }
        method_exchangeImplementations(a, b)
    }
}

// Each replacement calls itself, which after the exchange is the original.
extension CADisplayLink {
    @objc dynamic class func scaena_displayLink(withTarget target: Any, selector: Selector) -> CADisplayLink {
        let link = scaena_displayLink(withTarget: target, selector: selector)
        TimerReport.made(link, for: target, selector)
        return link
    }
}

extension RunLoop {
    @objc dynamic func scaena_add(_ timer: Timer, forMode mode: RunLoop.Mode) {
        if self == RunLoop.main { TimerReport.scheduled(timer.timeInterval, repeats: timer.timeInterval > 0) }
        scaena_add(timer, forMode: mode)
    }
}

extension Timer {
    @objc dynamic class func scaena_scheduledTimer(
        withTimeInterval interval: TimeInterval, repeats: Bool, block: @escaping @Sendable (Timer) -> Void
    ) -> Timer {
        TimerReport.scheduled(interval, repeats: repeats)
        return scaena_scheduledTimer(withTimeInterval: interval, repeats: repeats, block: block)
    }

    @objc dynamic class func scaena_scheduledTimer(
        timeInterval interval: TimeInterval, target: Any, selector: Selector, userInfo: Any?, repeats: Bool
    ) -> Timer {
        TimerReport.scheduled(interval, repeats: repeats)
        return scaena_scheduledTimer(
            timeInterval: interval, target: target, selector: selector, userInfo: userInfo, repeats: repeats)
    }
}
#endif
