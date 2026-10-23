import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The screen kept awake while a deck is presented (lift-up REL-1). A talk that stays on one slide
/// past the display's sleep timer would dim or blank the projector mid-sentence, and an iPad driven
/// from the Remote, never touched, would lock. On the Mac, an activity that holds off idle display
/// sleep and idle system sleep for as long as it is held; on the iPad, the idle timer off while any
/// is held. Taken as a show begins, and let go as it ends; a show ended by its window's close, by a
/// key, or by the remote lets it go all the same, through `Presenting.end`.
@MainActor
public final class Awake {
    /// How many are held: the iPad's idle timer is one switch for them all.
    public private(set) static var held = 0

    #if os(macOS)
    private var activity: (any NSObjectProtocol)?
    #else
    /// What turns the iPad's idle timer off (true) and on again: UIKit's. A test host's
    /// UIApplication, which shows no scene, reads the setting back as unset however often it is
    /// made, so a test watches the switch thrown here instead.
    public static var idleTimer: @MainActor (Bool) -> Void = { UIApplication.shared.isIdleTimerDisabled = $0 }
    /// The app watched for becoming active: UIKit puts the idle timer back as an app activates, so
    /// one held through a launch or a return from the background is set again.
    private static var activating: (any NSObjectProtocol)?
    #endif
    private var ended = false

    /// The screen kept awake, for `reason`, which the Mac's power management names.
    public init(reason: String) {
        #if os(macOS)
        activity = ProcessInfo.processInfo.beginActivity(options: [.idleDisplaySleepDisabled, .userInitiated], reason: reason)
        #endif
        Awake.held += 1
        Awake.apply()
    }

    /// Let the screen sleep again, once.
    public func end() {
        guard !ended else { return }
        ended = true
        Awake.held -= 1
        #if os(macOS)
        if let activity { ProcessInfo.processInfo.endActivity(activity) }
        activity = nil
        #endif
        Awake.apply()
    }

    /// The iPad's idle timer off while any is held, and on again once none is; nothing more on the
    /// Mac, whose activities each hold their own. UIKit drops a set made as the app comes up (the
    /// first one a test host makes reads back unset), so it is set again on the next turn of the
    /// main run loop, and as the app activates.
    private static func apply() {
        #if !os(macOS)
        idleTimer(held > 0)
        Task { @MainActor in idleTimer(held > 0) }
        if activating == nil {
            activating = NotificationCenter.default.addObserver(
                forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
            ) { _ in
                Task { @MainActor in Awake.apply() }
            }
        }
        #endif
    }
}

/// Where a show's windows go on the displays there are (PLAN 3.5, lift-up REL-1): the stage on a
/// display other than `home`, the one the deck is edited on, where there is one, and the presenter's
/// window on `home`; with `home` alone, the stage on it and no presenter's window. Read again as
/// displays come and go while the deck plays: a projector plugged in after Play takes the stage,
/// and one unplugged gives it back. A `home` that went away is the first display there is.
public struct Staging<Display: Equatable>: Equatable {
    /// The display the stage fills.
    public let stage: Display
    /// The display the presenter's window takes, where the stage is on another.
    public let presenter: Display?

    /// None where there is no display at all.
    public init?(home: Display?, among displays: [Display]) {
        guard let first = displays.first else { return nil }
        let home = home.flatMap { displays.contains($0) ? $0 : nil } ?? first
        let other = displays.first { $0 != home }
        stage = other ?? home
        presenter = other == nil ? nil : home
    }
}
