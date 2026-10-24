#if !os(macOS)
import ScaenaKit
import Testing
import UIKit

/// A deck being made on the iPad asks iPadOS for time in the background (lift-up REL-5): the time
/// is held while the work runs and handed back once, when it ends or when iPadOS gives no more,
/// which first says so.
@Test @MainActor
func aRunHoldsItsTimeInTheBackgroundAndHandsItBackOnce() {
    var asked: [String] = []
    var ended: [UIBackgroundTaskIdentifier] = []
    var expiring: (@MainActor () -> Void)?
    let system = KeptRunning.System(
        begin: { name, expired in
            asked.append(name)
            expiring = expired
            return UIBackgroundTaskIdentifier(rawValue: 7)
        },
        end: { ended.append($0) })

    // Ended as the work ends.
    var said = 0
    let done = KeptRunning("Making a deck", system: system) { said += 1 }
    #expect(asked == ["Making a deck"], "the time was not asked for")
    #expect(done.holding)
    done.end()
    done.end()
    #expect(ended == [UIBackgroundTaskIdentifier(rawValue: 7)], "the time was handed back \(ended.count) times")
    #expect(!done.holding)
    #expect(said == 0, "a run that ended said it was stopped")

    // Ended as iPadOS gives no more time: it says so, then hands the time back, once.
    ended = []
    let stopped = KeptRunning("Making a deck", system: system) { said += 1 }
    expiring?()
    #expect(said == 1, "the run did not say it was stopped")
    #expect(ended == [UIBackgroundTaskIdentifier(rawValue: 7)], "the time was not handed back as it expired")
    stopped.end()
    #expect(ended.count == 1, "the time was handed back again after it expired")
}
#endif
