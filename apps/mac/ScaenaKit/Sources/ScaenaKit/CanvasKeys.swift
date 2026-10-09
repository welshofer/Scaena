// The Mac's: the iPad's take `UITextInput`'s, in CanvasTextInput.swift (PLAN 4.4, ADR-0023).
#if os(macOS)
import AppKit
import SwiftUI

/// The canvas's keys (PLAN 3.9, 3.11): a view under the canvas that takes the keyboard when the
/// canvas is pressed, as the browser's canvas takes the focus. While a text is typed in, it takes
/// the keys and an input method's composition as the text input system gives them
/// (`NSTextInputClient`), and hands each to `Typing`; otherwise the commands the keys make are
/// the canvas's own (`command`): Delete, Shift+Delete, Escape. It lays nothing out and draws
/// nothing: it answers the input system from the engine's carets, and the canvas draws the caret
/// and the selection. It takes no press: the canvas reads each, and a press in the text typed in
/// puts the caret there, one outside it stops typing. Focus gone to the source stops typing too; a
/// field, the inspector's or the one ⌘K asks in, leaves it as it is, as the browser's inspector
/// does.
@MainActor
public final class CanvasKeys: NSView {
    public let typing: Typing
    /// The canvas, in canvas units: what the view's width is in points.
    public var canvas = CGSize(width: 1920, height: 1080)
    /// The part of the canvas the view shows, canvas units, where it is zoomed in (PLAN 3.16):
    /// none, the whole canvas.
    public var shown: CGRect?
    /// What the canvas does with the Edit menu's Find while no field takes it (PLAN 3.16): the
    /// deck's find bar opened, or its next or previous match shown.
    public var finding: (@MainActor (NSTextFinder.Action) -> Void)?
    /// What the canvas does with a key while no text is typed in, before the input system makes a
    /// command of it (PLAN 3.17): Tab, Return, Escape, the arrows, Space, the brackets, and +, as
    /// the browser's canvas reads them. Whether it took the key.
    public var pressed: (@MainActor (NSEvent) -> Bool)?
    /// What the canvas does with a command the keys make while no text is typed in: whether it did
    /// anything with it.
    public var command: (@MainActor (Selector) -> Bool)?
    /// What the canvas does with the Edit menu's Copy, Cut, and Paste while no text is typed in
    /// (PLAN 3.12): the nodes selected copied as a clip, or what the pasteboard holds pasted.
    public var clipping: (@MainActor (Clipping) -> Void)?

    public init(typing: Typing) {
        self.typing = typing
        super.init(frame: .zero)
        typing.focus = { [weak self] in self?.claim() }
        typing.discarded = { [weak self] in self?.inputContext?.discardMarkedText() }
    }

    public required init?(coder: NSCoder) { return nil }

    public override var isFlipped: Bool { true }
    public override var acceptsFirstResponder: Bool { true }

    /// Presses go through to the canvas under it.
    public override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Take the keyboard for the canvas, once the event that asked has gone by: a press on it, or
    /// a text entered.
    public func claim() {
        Task { @MainActor [weak self] in
            guard let self, let window = self.window, window.firstResponder !== self else { return }
            window.makeFirstResponder(self)
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
        // While no text is typed in, the canvas reads the key first (PLAN 3.17).
        if !typing.typing, pressed?(event) == true { return }
        // The input system makes the key text, a composition, or a command (`doCommand`): the
        // text's while one is typed in, else the canvas's.
        interpretKeyEvents([event])
    }

    /// ⌘B, ⌘I, ⌘K, ⌘A, ⌘⇧8, and ⌘⇧7 while typing, as the browser's text typed in answers them
    /// (PLAN 2.38, 2.40, 2.69, 2.70); and ⌘F, the deck's find bar.
    public override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
        // ⌘F with the canvas focused opens the deck's find bar (PLAN 3.16), whatever the menus hold.
        if let finding, window?.firstResponder === self, flags == .command,
            event.charactersIgnoringModifiers?.lowercased() == "f"
        {
            finding(.showFindInterface)
            return true
        }
        // ⌘A with the canvas focused and no text typed in: all beside the node selected (PLAN 3.17).
        if !typing.typing, let pressed, window?.firstResponder === self, flags == .command,
            event.charactersIgnoringModifiers?.lowercased() == "a", pressed(event)
        {
            return true
        }
        guard typing.typing, window?.firstResponder === self else { return super.performKeyEquivalent(with: event) }
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

    // The Edit menu's Copy, Cut, and Paste: on the characters selected, as plain text, while a
    // text is typed in; else the canvas's (`clipping`).

    @objc public func copy(_ sender: Any?) {
        guard typing.typing else { return clip(.copy) }
        guard let selected = typing.selected else { return }
        let board = NSPasteboard.general
        board.clearContents()
        board.setString(selected, forType: .string)
    }

    @objc public func cut(_ sender: Any?) {
        guard typing.typing else { return clip(.cut) }
        guard typing.selected != nil else { return }
        copy(sender)
        typing.replace(typing.selection, with: "")
    }

    @objc public func paste(_ sender: Any?) {
        guard typing.typing else { return clip(.paste) }
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
        typing.insert(text)
    }

    private func clip(_ what: Clipping) {
        clipping?(what)
    }

    /// The Edit menu's Find, Find Next, and Find Previous: the deck's find bar (PLAN 3.16), as ⌘F
    /// on the browser's canvas opens it, while a text is typed in too.
    public override func performTextFinderAction(_ sender: Any?) {
        let tag = (sender as? NSValidatedUserInterfaceItem)?.tag ?? NSTextFinder.Action.showFindInterface.rawValue
        guard let finding, let action = NSTextFinder.Action(rawValue: tag) else { return super.performTextFinderAction(sender) }
        finding(action)
    }

    /// The Find panel's older action, as some menus send it.
    @objc public func performFindPanelAction(_ sender: Any?) {
        performTextFinderAction(sender)
    }

    /// The commands the input system makes of keys: while a text is typed in, deletes, moves, a
    /// new line, and Escape; else the canvas's (`command`).
    public override func doCommand(by selector: Selector) {
        typealias Keys = NSStandardKeyBindingResponding
        guard typing.typing else {
            guard command?(selector) != true else { return }
            // Tab and Shift+Tab the canvas does not take pass the keyboard on, as anywhere.
            if selector == #selector(Keys.insertTab(_:)) { window?.selectNextKeyView(self) }
            if selector == #selector(Keys.insertBacktab(_:)) { window?.selectPreviousKeyView(self) }
            return
        }
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
            if typing.endsList { typing.list(kind: "none", done: "The list ends") } else { typing.insert("\n") }
        case #selector(Keys.insertLineBreak(_:)), #selector(Keys.insertParagraphSeparator(_:)),
            #selector(Keys.insertNewlineIgnoringFieldEditor(_:)):
            typing.insert("\n")
        case #selector(Keys.insertTab(_:)), #selector(Keys.insertBacktab(_:)):
            // In a list, the items selected a level in, or out; elsewhere a text's field gives Tab
            // to what is next, as the browser's does: typing stops.
            let out = selector == #selector(Keys.insertBacktab(_:))
            if typing.inList {
                typing.list(by: out ? -1 : 1, done: out ? "A level out" : "A level in")
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
    fileprivate var scale: CGFloat { bounds.width / max(shown?.width ?? canvas.width, 1) }

    /// The canvas point at the view's top left.
    fileprivate var origin: CGPoint { shown?.origin ?? .zero }
}

extension CanvasKeys: @preconcurrency NSTextInputClient {
    public func insertText(_ string: Any, replacementRange: NSRange) {
        // Characters typed with no text typed in do nothing yet.
        guard typing.typing else { return }
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
        guard typing.typing else { return }
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
            x: (box.minX - origin.x) * scale, y: (box.minY - origin.y) * scale, width: max(box.width * scale, 1),
            height: box.height * scale)
        return window.convertToScreen(convert(local, to: nil))
    }

    public func characterIndex(for point: NSPoint) -> Int {
        guard let window, scale > 0 else { return NSNotFound }
        let local = convert(window.convertPoint(fromScreen: point), from: nil)
        return typing.offset(near: CGPoint(x: local.x / scale + origin.x, y: local.y / scale + origin.y))
    }

    public func attributedString() -> NSAttributedString {
        NSAttributedString(string: typing.carets?.text ?? "")
    }
}

/// `CanvasKeys` in SwiftUI, under the canvas: the keys of the canvas pressed last, `typing`'s
/// while it types in a text, else what `command` does with them.
public struct CanvasKeysHost: NSViewRepresentable {
    public let typing: Typing
    /// The canvas, in canvas units.
    public let canvas: CGSize
    /// The part of it shown, where it is zoomed in.
    public let shown: CGRect?
    public let command: @MainActor (Selector) -> Bool
    public let clipping: @MainActor (Clipping) -> Void
    public let finding: (@MainActor (NSTextFinder.Action) -> Void)?
    public let pressed: (@MainActor (NSEvent) -> Bool)?

    public init(
        typing: Typing, canvas: CGSize, shown: CGRect? = nil, command: @escaping @MainActor (Selector) -> Bool,
        clipping: @escaping @MainActor (Clipping) -> Void,
        finding: (@MainActor (NSTextFinder.Action) -> Void)? = nil, pressed: (@MainActor (NSEvent) -> Bool)? = nil
    ) {
        self.typing = typing
        self.canvas = canvas
        self.shown = shown
        self.command = command
        self.clipping = clipping
        self.finding = finding
        self.pressed = pressed
    }

    public func makeNSView(context: Context) -> CanvasKeys {
        let view = CanvasKeys(typing: typing)
        updateNSView(view, context: context)
        return view
    }

    public func updateNSView(_ view: CanvasKeys, context: Context) {
        view.canvas = canvas
        view.shown = shown
        view.command = command
        view.clipping = clipping
        view.finding = finding
        view.pressed = pressed
    }
}
#endif
