#if os(macOS)
import AppKit
import ScaenaKit
import Testing

/// Undo held while the assistant works is let go however its views come and go (lift-up STAB-5):
/// a window carries a hold once, a second hold stands before the first, and the first, taken out,
/// leaves wherever it stands, so a hold left locked never refuses ⌘Z once the assistant is done.
@MainActor
@Test func anUndoHoldIsLetGoWhereverItStandsInTheChain() {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 200, height: 100), styleMask: [.titled], backing: .buffered,
        defer: true)
    let content = NSView()
    window.contentView = content
    var refused: [String] = []
    let first = UndoHold()
    first.refused = { refused.append("first") }
    let second = UndoHold()
    second.refused = { refused.append("second") }
    let undo = #selector(UndoHold.undo(_:))

    // The first held and locked, as the assistant works; then SwiftUI makes the second before it
    // takes the first away.
    first.hold(in: window)
    first.hold(in: window)
    #expect(UndoHold.chain(after: content).first === first, "held after the content")
    #expect(UndoHold.chain(after: content).filter { $0 === first }.count == 1, "and held once")
    first.locked = true
    second.hold(in: window)
    let both = UndoHold.chain(after: content)
    #expect(both.count > 2 && both[0] === second && both[1] === first, "the second stands before the first")
    #expect(content.tryToPerform(undo, with: nil) && refused == ["first"], "the locked one, past the unlocked one")

    // The first taken away from behind the second: the chain runs on to the window.
    first.release(from: window)
    #expect(!UndoHold.chain(after: content).contains { $0 === first })
    #expect(UndoHold.chain(after: content).first === second)
    #expect(UndoHold.chain(after: second).first === window, "the window after the hold that stays")
    refused = []
    _ = content.tryToPerform(undo, with: nil)
    #expect(refused.isEmpty, "unlocked, the hold that stays passes Undo on")

    // Locked again, it holds; let go, the content leads to the window again.
    second.locked = true
    #expect(content.tryToPerform(undo, with: nil) && refused == ["second"])
    second.release(from: window)
    #expect(UndoHold.chain(after: content).first === window)
}
#endif
