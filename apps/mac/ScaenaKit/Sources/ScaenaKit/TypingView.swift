import AppKit
import SwiftUI

/// Where keys go while a text is typed in on the canvas (PLAN 3.9): a view over the canvas that
/// takes the keyboard, and an input method's composition, as the text input system gives them
/// (`NSTextInputClient`), and hands each to `Typing`. It lays nothing out and draws nothing: it
/// answers the input system from the engine's carets, and the canvas draws the caret and the
/// selection. It takes no press: the canvas reads each, as the browser's does, and a press in the
/// text typed in puts the caret there, one outside it stops typing. Focus gone to the source stops
/// it too; a field, the inspector's or the one ⌘K asks in, leaves it as it is, as the browser's
/// inspector does.
@MainActor
public final class TypingView: NSView {
    public let typing: Typing
    /// The canvas, in canvas units: what the view's width is in points.
    public var canvas = CGSize(width: 1920, height: 1080)

    public init(typing: Typing) {
        self.typing = typing
        super.init(frame: .zero)
        typing.focus = { [weak self] in self?.claim() }
        typing.discarded = { [weak self] in self?.inputContext?.discardMarkedText() }
    }

    public required init?(coder: NSCoder) { return nil }

    public override var isFlipped: Bool { true }
    public override var acceptsFirstResponder: Bool { typing.typing }

    /// Presses go through to the canvas under it.
    public override func hitTest(_ point: NSPoint) -> NSView? { nil }

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        take()
    }

    /// Take the keyboard while a text is typed in, and give it back when none is, once the event
    /// that asked has gone by.
    public func claim() {
        Task { @MainActor [weak self] in self?.take() }
    }

    private func take() {
        guard let window else { return }
        if typing.typing {
            if window.firstResponder !== self { window.makeFirstResponder(self) }
        } else if window.firstResponder === self {
            window.makeFirstResponder(nil)
        }
    }

    public override func resignFirstResponder() -> Bool {
        // Focus gone to the source, an editor of its own, stops typing, as in the browser; a
        // field (its editor is the window's field editor) leaves it as it is.
        Task { @MainActor [weak self] in
            guard let self, let window = self.window, window.firstResponder !== self else { return }
            if let text = window.firstResponder as? NSText, !text.isFieldEditor { self.typing.leave() }
        }
        return true
    }

    public override func keyDown(with event: NSEvent) {
        guard typing.typing else { return super.keyDown(with: event) }
        // The input system makes the key text, a composition, or a command (`doCommand`).
        interpretKeyEvents([event])
    }

    /// ⌘B, ⌘I, ⌘K, ⌘A, ⌘⇧8, and ⌘⇧7 while typing, as the browser's text typed in answers them
    /// (PLAN 2.38, 2.40, 2.69, 2.70).
    public override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard typing.typing, window?.firstResponder === self else { return super.performKeyEquivalent(with: event) }
        let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
        if flags == [.command, .shift] {
            // By the keys' places, as Shift makes them other characters: the 8 key and the 7.
            switch event.keyCode {
            case 28: typing.toggle("bullet")
            case 26: typing.toggle("number")
            default: return super.performKeyEquivalent(with: event)
            }
            return true
        }
        guard flags == .command else { return super.performKeyEquivalent(with: event) }
        switch event.charactersIgnoringModifiers?.lowercased() {
        case "b": typing.bold()
        case "i": typing.italic()
        case "k": typing.askLink()
        case "a": typing.selectAll()
        default: return super.performKeyEquivalent(with: event)
        }
        return true
    }

    // The Edit menu's Copy, Cut, and Paste, on the characters selected: plain text.

    @objc public func copy(_ sender: Any?) {
        guard let selected = typing.selected else { return }
        let board = NSPasteboard.general
        board.clearContents()
        board.setString(selected, forType: .string)
    }

    @objc public func cut(_ sender: Any?) {
        guard typing.selected != nil else { return }
        copy(sender)
        typing.replace(typing.selection, with: "")
    }

    @objc public func paste(_ sender: Any?) {
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
        typing.insert(text)
    }

    /// The commands the input system makes of keys: deletes, moves, a new line, and Escape.
    public override func doCommand(by selector: Selector) {
        typealias Keys = NSStandardKeyBindingResponding
        switch selector {
        case #selector(Keys.deleteBackward(_:)), #selector(Keys.deleteBackwardByDecomposingPreviousCharacter(_:)):
            typing.delete(backward: true, by: .character)
        case #selector(Keys.deleteForward(_:)):
            typing.delete(backward: false, by: .character)
        case #selector(Keys.deleteWordBackward(_:)):
            typing.delete(backward: true, by: .word)
        case #selector(Keys.deleteWordForward(_:)):
            typing.delete(backward: false, by: .word)
        case #selector(Keys.deleteToBeginningOfLine(_:)), #selector(Keys.deleteToBeginningOfParagraph(_:)):
            typing.delete(backward: true, by: .line)
        case #selector(Keys.deleteToEndOfLine(_:)), #selector(Keys.deleteToEndOfParagraph(_:)):
            typing.delete(backward: false, by: .line)
        case #selector(Keys.insertNewline(_:)):
            // In an empty item, the list ends there (ADR-0018); else a new paragraph, an item like
            // the one it leaves in a list.
            if typing.endsList { typing.list(kind: "none", done: "the list ends") } else { typing.insert("\n") }
        case #selector(Keys.insertLineBreak(_:)), #selector(Keys.insertParagraphSeparator(_:)),
            #selector(Keys.insertNewlineIgnoringFieldEditor(_:)):
            typing.insert("\n")
        case #selector(Keys.insertTab(_:)), #selector(Keys.insertBacktab(_:)):
            // In a list, the items selected a level in, or out; elsewhere a text's field gives Tab
            // to what is next, as the browser's does: typing stops.
            let out = selector == #selector(Keys.insertBacktab(_:))
            if typing.inList {
                typing.list(by: out ? -1 : 1, done: out ? "a level out" : "a level in")
            } else {
                typing.leave()
            }
        case #selector(Keys.cancelOperation(_:)):
            typing.leave()
        case #selector(Keys.selectAll(_:)):
            typing.selectAll()
        default:
            if let move = Self.moves[selector] { typing.move(move.step, extend: move.extend) }
        }
    }

    /// Each move the input system names: how far it goes, and whether it extends the selection.
    private static let moves: [Selector: (step: Typing.Step, extend: Bool)] = {
        typealias Keys = NSStandardKeyBindingResponding
        return [
            #selector(Keys.moveLeft(_:)): (.character(-1), false),
            #selector(Keys.moveRight(_:)): (.character(1), false),
            #selector(Keys.moveBackward(_:)): (.character(-1), false),
            #selector(Keys.moveForward(_:)): (.character(1), false),
            #selector(Keys.moveLeftAndModifySelection(_:)): (.character(-1), true),
            #selector(Keys.moveRightAndModifySelection(_:)): (.character(1), true),
            #selector(Keys.moveBackwardAndModifySelection(_:)): (.character(-1), true),
            #selector(Keys.moveForwardAndModifySelection(_:)): (.character(1), true),
            #selector(Keys.moveWordLeft(_:)): (.word(-1), false),
            #selector(Keys.moveWordRight(_:)): (.word(1), false),
            #selector(Keys.moveWordBackward(_:)): (.word(-1), false),
            #selector(Keys.moveWordForward(_:)): (.word(1), false),
            #selector(Keys.moveWordLeftAndModifySelection(_:)): (.word(-1), true),
            #selector(Keys.moveWordRightAndModifySelection(_:)): (.word(1), true),
            #selector(Keys.moveWordBackwardAndModifySelection(_:)): (.word(-1), true),
            #selector(Keys.moveWordForwardAndModifySelection(_:)): (.word(1), true),
            #selector(Keys.moveUp(_:)): (.line(-1), false),
            #selector(Keys.moveDown(_:)): (.line(1), false),
            #selector(Keys.moveUpAndModifySelection(_:)): (.line(-1), true),
            #selector(Keys.moveDownAndModifySelection(_:)): (.line(1), true),
            #selector(Keys.moveToBeginningOfLine(_:)): (.lineEdge(start: true), false),
            #selector(Keys.moveToEndOfLine(_:)): (.lineEdge(start: false), false),
            #selector(Keys.moveToLeftEndOfLine(_:)): (.lineEdge(start: true), false),
            #selector(Keys.moveToRightEndOfLine(_:)): (.lineEdge(start: false), false),
            #selector(Keys.moveToBeginningOfLineAndModifySelection(_:)): (.lineEdge(start: true), true),
            #selector(Keys.moveToEndOfLineAndModifySelection(_:)): (.lineEdge(start: false), true),
            #selector(Keys.moveToLeftEndOfLineAndModifySelection(_:)): (.lineEdge(start: true), true),
            #selector(Keys.moveToRightEndOfLineAndModifySelection(_:)): (.lineEdge(start: false), true),
            #selector(Keys.moveToBeginningOfParagraph(_:)): (.lineEdge(start: true), false),
            #selector(Keys.moveToEndOfParagraph(_:)): (.lineEdge(start: false), false),
            #selector(Keys.moveToBeginningOfDocument(_:)): (.text(start: true), false),
            #selector(Keys.moveToEndOfDocument(_:)): (.text(start: false), false),
            #selector(Keys.moveToBeginningOfDocumentAndModifySelection(_:)): (.text(start: true), true),
            #selector(Keys.moveToEndOfDocumentAndModifySelection(_:)): (.text(start: false), true),
            #selector(Keys.scrollToBeginningOfDocument(_:)): (.text(start: true), false),
            #selector(Keys.scrollToEndOfDocument(_:)): (.text(start: false), false),
        ]
    }()

    /// `string` as the input system hands it, a string or one with attributes: its characters.
    fileprivate static func plain(_ string: Any) -> String {
        if let attributed = string as? NSAttributedString { return attributed.string }
        return string as? String ?? ""
    }

    /// Points on the view to the canvas unit.
    fileprivate var scale: CGFloat { bounds.width / max(canvas.width, 1) }
}

extension TypingView: @preconcurrency NSTextInputClient {
    public func insertText(_ string: Any, replacementRange: NSRange) {
        let text = Self.plain(string)
        if replacementRange.location == NSNotFound {
            typing.insert(text)
        } else {
            // What an input method replaces: an accent chosen for the letter before, say.
            typing.unmark()
            typing.replace(replacementRange, with: text)
        }
        inputContext?.invalidateCharacterCoordinates()
    }

    public func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        if replacementRange.location != NSNotFound {
            // The input method composes again what is written there.
            typing.unmark()
            typing.select(replacementRange)
        }
        typing.mark(Self.plain(string), selected: selectedRange)
        inputContext?.invalidateCharacterCoordinates()
    }

    public func unmarkText() {
        typing.unmark()
    }

    public func selectedRange() -> NSRange {
        typing.typing ? typing.selection : NSRange(location: NSNotFound, length: 0)
    }

    public func markedRange() -> NSRange {
        typing.marked ?? NSRange(location: NSNotFound, length: 0)
    }

    public func hasMarkedText() -> Bool {
        typing.marked != nil
    }

    public func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        guard let text = typing.carets?.text, range.location != NSNotFound else { return nil }
        let whole = text as NSString
        guard range.location <= whole.length else { return nil }
        let found = NSRange(location: range.location, length: min(max(range.length, 0), whole.length - range.location))
        actualRange?.pointee = found
        return NSAttributedString(string: whole.substring(with: found))
    }

    public func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    /// Where the characters of `range` are drawn, on the screen: where an input method puts its
    /// window. The caret's line box where the range begins, from the engine's carets.
    public func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        let at = range.location == NSNotFound ? typing.head : range.location
        actualRange?.pointee = NSRange(location: at, length: 0)
        guard let box = typing.caretBox(at: at), let window else { return .zero }
        let local = NSRect(
            x: box.minX * scale, y: box.minY * scale, width: max(box.width * scale, 1), height: box.height * scale)
        return window.convertToScreen(convert(local, to: nil))
    }

    public func characterIndex(for point: NSPoint) -> Int {
        guard let window, scale > 0 else { return NSNotFound }
        let local = convert(window.convertPoint(fromScreen: point), from: nil)
        return typing.offset(near: CGPoint(x: local.x / scale, y: local.y / scale))
    }

    public func attributedString() -> NSAttributedString {
        NSAttributedString(string: typing.carets?.text ?? "")
    }
}

/// `TypingView` in SwiftUI, over the canvas: it takes the keyboard while `typing` types in a
/// text, and gives it back when it stops.
public struct TypingHost: NSViewRepresentable {
    public let typing: Typing
    /// Whether a text is typed in: what takes the keyboard, or gives it back.
    public let active: Bool
    /// The canvas, in canvas units.
    public let canvas: CGSize

    public init(typing: Typing, canvas: CGSize) {
        self.typing = typing
        self.active = typing.typing
        self.canvas = canvas
    }

    public func makeNSView(context: Context) -> TypingView {
        let view = TypingView(typing: typing)
        view.canvas = canvas
        return view
    }

    public func updateNSView(_ view: TypingView, context: Context) {
        view.canvas = canvas
        // Typing begun takes the keyboard itself (`Typing.focus`); stopped, it gives it back.
        if !active { view.claim() }
    }
}
