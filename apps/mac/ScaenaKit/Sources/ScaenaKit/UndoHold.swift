#if os(macOS)
import AppKit

/// Undo and Redo held while the assistant is at work on the deck (lift-up STAB-5), as the browser
/// holds them: a responder put into a window's chain after its content, which takes the Edit
/// menu's `undo:` and `redo:` while `locked` and says why instead. Unlocked, it answers neither,
/// so they go on to the window's own undo as before. The app's `UndoGuard` holds one in each
/// deck's window.
@MainActor
public final class UndoHold: NSResponder {
    /// Whether Undo and Redo are held.
    public var locked = false
    /// Told of an undo or a redo held while `locked`.
    public var refused: () -> Void = {}

    // NSObject's, so not the main actor's; AppKit asks it on the main thread as it walks the chain.
    override nonisolated public func responds(to selector: Selector!) -> Bool {
        if selector == #selector(undo(_:)) || selector == #selector(redo(_:)) {
            return MainActor.assumeIsolated { locked }
        }
        return super.responds(to: selector)
    }

    @objc public func undo(_ sender: Any?) { refused() }

    @objc public func redo(_ sender: Any?) { refused() }

    /// Put after `window`'s content view, where it is not in the chain already: a window carries
    /// it once.
    public func hold(in window: NSWindow) {
        guard let content = window.contentView, !Self.chain(after: content).contains(where: { $0 === self }) else {
            return
        }
        nextResponder = content.nextResponder
        content.nextResponder = self
    }

    /// Taken out of `window`'s chain wherever it stands there. Another hold made after it (by a
    /// view SwiftUI made before it took this one's away) stands before it, so it is found by
    /// walking the chain, never only right after the content view.
    public func release(from window: NSWindow) {
        guard let content = window.contentView else { return }
        let before = [content] + Self.chain(after: content)
        guard let link = before.first(where: { $0.nextResponder === self }) else { return }
        link.nextResponder = nextResponder
        nextResponder = nil
    }

    /// The responders after `start`, in order, as far as the chain goes: at most 64, should one
    /// ever come round again.
    public static func chain(after start: NSResponder) -> [NSResponder] {
        var found: [NSResponder] = []
        var link = start.nextResponder
        while let current = link, found.count < 64 {
            found.append(current)
            link = current.nextResponder
        }
        return found
    }
}
#endif
