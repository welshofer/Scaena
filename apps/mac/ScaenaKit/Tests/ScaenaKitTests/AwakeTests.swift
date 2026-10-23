import Foundation
import ScaenaKit
import Testing

#if os(macOS)
import IOKit.pwr_mgt
#else
import UIKit
#endif

#if os(macOS)

/// Whether this process holds off the display's idle sleep, as the system's power management
/// sees it: an assertion of its own of the kind `pmset -g assertions` lists as
/// PreventUserIdleDisplaySleep.
private func holdsOffDisplaySleep() -> Bool {
    var held: Unmanaged<CFDictionary>?
    guard IOPMCopyAssertionsByProcess(&held) == kIOReturnSuccess, let held else { return false }
    let byProcess = held.takeRetainedValue() as NSDictionary
    let pid = NSNumber(value: getpid())
    guard let ours = byProcess[pid] as? [[String: Any]] else { return false }
    return ours.contains { $0[kIOPMAssertionTypeKey as String] as? String == kIOPMAssertionTypePreventUserIdleDisplaySleep as String }
}

/// Whether `holds()` comes to `expected` within two seconds: the system takes the assertion a
/// moment after it is asked for.
private func soon(_ expected: Bool, _ holds: () -> Bool) -> Bool {
    let until = Date().addingTimeInterval(2)
    while Date() < until {
        if holds() == expected { return true }
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
    }
    return holds() == expected
}
#endif

/// The screen stays awake while a show runs (lift-up REL-1), and may sleep again once it ends. On
/// the Mac, the process holds off the display's idle sleep for exactly as long as an `Awake` is
/// held, as the system's power management counts it; on the iPad, the idle timer is off while any
/// is held, and on again once none is.
@Test @MainActor func theScreenStaysAwakeWhileAShowRuns() {
    #if os(macOS)
    #expect(soon(false, holdsOffDisplaySleep), "before: nothing holds the display awake")
    #endif
    let before = Awake.held
    let show = Awake(reason: "A test presents a deck")
    #expect(Awake.held == before + 1)
    #if os(macOS)
    #expect(soon(true, holdsOffDisplaySleep), "while a show runs, the display stays awake")
    #else
    #expect(UIApplication.shared.isIdleTimerDisabled, "while a show runs, the iPad does not lock")
    #endif
    // A second holder, as the remote's window beside a show: the screen stays awake until both end.
    let second = Awake(reason: "A second holder")
    show.end()
    show.end()
    #expect(Awake.held == before + 1, "ended once, however often it is told")
    #if os(macOS)
    #expect(soon(true, holdsOffDisplaySleep), "the second still holds the display awake")
    #else
    #expect(UIApplication.shared.isIdleTimerDisabled)
    #endif
    second.end()
    #expect(Awake.held == before)
    #if os(macOS)
    #expect(soon(false, holdsOffDisplaySleep), "every show ended, the display may sleep")
    #else
    #expect(!UIApplication.shared.isIdleTimerDisabled, "every show ended, the iPad may lock")
    #endif
}

/// Where the stage goes (PLAN 3.5, lift-up REL-1): with the display the deck is edited on alone, on
/// it, and no presenter's window; with another, the stage on the other and the presenter's window
/// on home. Read again as displays come and go: a projector plugged in after Play takes the stage,
/// one unplugged gives it back, and home gone, the first display there is stands in for it.
@Test func theStageFollowsTheDisplaysThatComeAndGo() {
    /// The stage and the presenter's display, as `Staging` places them, or nil for no display.
    func placed(home: String?, among displays: [String]) -> [String?]? {
        Staging(home: home, among: displays).map { [$0.stage, $0.presenter] }
    }
    #expect(placed(home: "built-in", among: ["built-in"]) == ["built-in", nil], "alone, the stage is home, and no presenter's window")
    #expect(placed(home: "built-in", among: ["built-in", "projector"]) == ["projector", "built-in"], "plugged in, a projector takes the stage")
    #expect(placed(home: "built-in", among: ["projector", "built-in"]) == ["projector", "built-in"], "wherever the system lists it")
    #expect(placed(home: "built-in", among: ["built-in"]) == ["built-in", nil], "unplugged, it gives the stage back")
    #expect(placed(home: "built-in", among: ["projector"]) == ["projector", nil], "home gone, the display there is stands in for it")
    #expect(placed(home: nil, among: ["built-in", "projector"]) == ["projector", "built-in"], "no home: the first is")
    #expect(placed(home: "built-in", among: []) == nil, "no display, no stage")
}
