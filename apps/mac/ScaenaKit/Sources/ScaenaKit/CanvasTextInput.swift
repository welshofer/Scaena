// The iPad's: the Mac's canvas keys take `NSTextInputClient`'s, in CanvasKeys.swift (PLAN 3.9, 4.4,
// ADR-0023).
#if os(iOS)
import GameController
import Observation
import SwiftUI
import UIKit
import os

/// What the canvas's keys do, at the debug level, which nothing keeps unless asked: the iPad's UI
/// tests stream it to say where a key went (`apps/ipad/app-test.sh`, PLAN 4.6).
private let keysLog = Logger(subsystem: "com.welshofer.Scaena", category: "keys")

/// The canvas's keys on the iPad (PLAN 4.4), as the Mac's are (PLAN 3.9, 3.10): a view under the
/// canvas that takes the keyboard while a text is typed in, and hands `Typing` what the text input
/// system makes of each key (`UITextInput`): the software keyboard's and a hardware keyboard's, an
/// input method's composition, dictation, and what the Pencil writes (Scribble). It lays nothing
/// out and draws nothing: it answers the input system from the engine's carets, and the canvas
/// draws the caret and the selection. It takes no touch: the canvas reads each, and a press in the
/// text typed in puts the caret there, one outside it stops typing.
///
/// - A hardware keyboard's arrows, deletes, Escape, and Tab do what the Mac's key bindings make of
///   them (`Typing.Step`, `Typing.Unit`); ⌘B, ⌘I, ⌘K, ⌘⇧8, and ⌘⇧7 give the Mac's looks, link,
///   and lists; ⌘F opens the deck's find bar.
/// - Writing with the Pencil on a text begins typing in it there (`UIIndirectScribbleInteraction`):
///   the state shown's texts are what Scribble writes in.
/// - The input system is told of each change it did not make itself: a press, a key's move, a look.
/// - While no text is typed in, the keyboard is the canvas's own (PLAN 4.6): a press on the canvas
///   gives it to `presses`, as does typing stopped where this view had it.
@MainActor
public final class CanvasKeys: UIView {
    public let typing: Typing
    /// The canvas, in canvas units: what the view's width is in points.
    public var canvas = CGSize(width: 1920, height: 1080)
    /// The part of the canvas the view shows, canvas units, where it is zoomed in (PLAN 3.16):
    /// none, the whole canvas.
    public var shown: CGRect?
    /// ⌘F: the deck's find bar (PLAN 3.16), while a text is typed in too.
    public var finding: (@MainActor () -> Void)?
    /// What the canvas does with Copy, Cut, and Paste while no text is typed in (PLAN 3.12): the
    /// nodes selected copied as a clip, or what the pasteboard holds pasted.
    public var clipping: (@MainActor (Clipping) -> Void)?
    /// The canvas's keys while no text is typed in (PLAN 4.6).
    public let presses = CanvasPresses()

    /// Told of each change the input system did not make.
    public weak var inputDelegate: (any UITextInputDelegate)?
    public var markedTextStyle: [NSAttributedString.Key: Any]?
    /// The text's lines as the engine set them; its words and sentences as the system finds them.
    public private(set) lazy var tokenizer: any UITextInputTokenizer = Boundaries(keys: self)

    /// The text, the selection, and the composition as the input system last saw them.
    private var seen: Seen?
    /// Whether the input system's own change is under way: it is told of none.
    private var asked = false
    /// The presses that stopped typing: how each goes on and ends is this view's too.
    private var stopped: Set<UIPress> = []

    public init(typing: Typing) {
        self.typing = typing
        super.init(frame: .zero)
        backgroundColor = .clear
        typing.focus = { [weak self] in self?.claim() }
        typing.discarded = { [weak self] in self?.changed() }
        addInteraction(UIIndirectScribbleInteraction(delegate: self))
        addSubview(presses)
        watch()
    }

    public required init?(coder: NSCoder) { return nil }

    /// Touches go through to the canvas over it.
    public override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? { nil }

    // MARK: The keyboard

    /// Only while a text is typed in: with the keyboard comes the software one.
    public override var canBecomeFirstResponder: Bool { typing.typing }

    /// Take the keyboard once the touch that asked has gone by: for the text typed in, the
    /// software keyboard with it; else for the canvas's own keys (PLAN 4.6), as a press on the
    /// Mac's canvas takes it.
    public func claim() {
        Task { @MainActor [weak self] in
            guard let self, self.window != nil else { return }
            if self.typing.typing {
                if !self.isFirstResponder { self.becomeFirstResponder() }
            } else if !self.presses.isFirstResponder {
                self.presses.becomeFirstResponder()
            }
        }
    }

    public override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        keysLog.debug("text input resigned: \(resigned)")
        // Focus gone to the source, an editor of its own (a text input that scrolls), stops typing,
        // as on the Mac; a field (the inspector's, or the one ⌘K asks in) leaves it as it is.
        Task { @MainActor [weak self] in
            guard let self, self.typing.typing, !self.isFirstResponder, let window = self.window,
                let taker = Self.responder(in: window), taker is UITextInput, taker is UIScrollView
            else { return }
            self.typing.leave()
        }
        return resigned
    }

    /// The view in `view` that has the keyboard, if one does.
    fileprivate static func responder(in view: UIView) -> UIView? {
        if view.isFirstResponder { return view }
        for inside in view.subviews {
            if let found = responder(in: inside) { return found }
        }
        return nil
    }

    /// A hardware keyboard's keys while a text is typed in, taken before the system's own text
    /// keys (`wantsPriorityOverSystemBehavior`).
    public override var keyCommands: [UIKeyCommand]? {
        typing.typing ? Self.commands : nil
    }

    /// Escape, and ⌘., the system's cancel key, stop typing where the system hands either on as a
    /// press rather than as a key command (PLAN 4.6), as the canvas's keys take each.
    public override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var rest = Set<UIPress>()
        for press in presses {
            if typing.typing, let key = press.key, Self.cancels(key) {
                stopped.insert(press)
                typing.leave()
            } else {
                rest.insert(press)
            }
        }
        if !rest.isEmpty { super.pressesBegan(rest, with: event) }
    }

    public override func pressesChanged(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.subtracting(stopped)
        if !rest.isEmpty { super.pressesChanged(rest, with: event) }
    }

    public override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.subtracting(stopped)
        stopped.subtract(presses)
        if !rest.isEmpty { super.pressesEnded(rest, with: event) }
    }

    public override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.subtracting(stopped)
        stopped.subtract(presses)
        if !rest.isEmpty { super.pressesCancelled(rest, with: event) }
    }

    /// Whether `key` is Escape, or ⌘. alone.
    private static func cancels(_ key: UIKey) -> Bool {
        let flags = key.modifierFlags.intersection([.command, .shift, .alternate, .control])
        return key.keyCode == .keyboardEscape && flags.isEmpty || key.keyCode == .keyboardPeriod && flags == .command
    }

    @objc private func keyed(_ command: UIKeyCommand) {
        guard let input = command.input else { return }
        _ = key(input, flags: command.modifierFlags)
    }

    /// A key of a hardware keyboard, `input` with `flags`, as the Mac's key bindings make it while
    /// a text is typed in: whether it did anything.
    @discardableResult
    public func key(_ input: String, flags: UIKeyModifierFlags = []) -> Bool {
        guard typing.typing, let found = Self.keys.first(where: { $0.input == input && $0.flags == flags }) else {
            return false
        }
        switch found.does {
        case .move(let step, let extend):
            typing.move(step, extend: extend)
        case .delete(let backward, let unit):
            typing.delete(backward: backward, by: unit)
        case .leave:
            typing.leave()
        case .tab(let back):
            tab(back: back)
        case .bold:
            typing.bold()
        case .italic:
            typing.italic()
        case .link:
            typing.askLink()
        case .list(let kind):
            typing.toggle(kind)
        case .find:
            finding?()
        }
        return true
    }

    /// Tab and Shift+Tab: in a list, the items selected a level in, or out; elsewhere a text's
    /// field gives Tab to what is next, as the browser's does: typing stops.
    private func tab(back: Bool) {
        if typing.inList {
            typing.list(by: back ? -1 : 1, done: back ? "A level out" : "A level in")
        } else {
            typing.leave()
        }
    }

    /// What a key does while a text is typed in.
    private enum Does {
        case move(Typing.Step, extend: Bool)
        case delete(backward: Bool, Typing.Unit)
        case leave, bold, italic, link, find
        case tab(back: Bool)
        case list(String)
    }

    private struct Key {
        let input: String
        let flags: UIKeyModifierFlags
        let does: Does
        /// What the keyboard's list of shortcuts calls it, if it lists it.
        var title = ""
    }

    /// The keys, as the Mac's key bindings make them (`CanvasKeys.doCommand(by:)` there): the
    /// arrows by character, by word with ⌥, to the line's edge with ⌘ (up and down: the text's),
    /// each extending the selection with ⇧; ⌥ and ⌘ with Delete, a word and the line's start.
    private static let keys: [Key] = {
        let (left, right) = (UIKeyCommand.inputLeftArrow, UIKeyCommand.inputRightArrow)
        let (up, down) = (UIKeyCommand.inputUpArrow, UIKeyCommand.inputDownArrow)
        var keys: [Key] = []
        func move(_ input: String, _ flags: UIKeyModifierFlags, _ step: Typing.Step) {
            keys.append(Key(input: input, flags: flags, does: .move(step, extend: false)))
            keys.append(Key(input: input, flags: flags.union(.shift), does: .move(step, extend: true)))
        }
        move(left, [], .character(-1))
        move(right, [], .character(1))
        move(left, .alternate, .word(-1))
        move(right, .alternate, .word(1))
        move(left, .command, .lineEdge(start: true))
        move(right, .command, .lineEdge(start: false))
        move(up, [], .line(-1))
        move(down, [], .line(1))
        move(up, .command, .text(start: true))
        move(down, .command, .text(start: false))
        keys.append(Key(input: up, flags: .alternate, does: .move(.lineEdge(start: true), extend: false)))
        keys.append(Key(input: down, flags: .alternate, does: .move(.lineEdge(start: false), extend: false)))
        // Delete is the backspace key, U+0008, as a hardware keyboard sends it.
        keys += [
            Key(input: "\u{8}", flags: .alternate, does: .delete(backward: true, .word)),
            Key(input: "\u{8}", flags: .command, does: .delete(backward: true, .line)),
            Key(input: UIKeyCommand.inputEscape, flags: [], does: .leave),
            Key(input: ".", flags: .command, does: .leave),
            Key(input: "\t", flags: [], does: .tab(back: false)),
            Key(input: "\t", flags: .shift, does: .tab(back: true)),
            Key(input: "b", flags: .command, does: .bold, title: "Bold"),
            Key(input: "i", flags: .command, does: .italic, title: "Italic"),
            Key(input: "k", flags: .command, does: .link, title: "Link"),
            Key(input: "8", flags: [.command, .shift], does: .list("bullet"), title: "Bulleted List"),
            Key(input: "7", flags: [.command, .shift], does: .list("number"), title: "Numbered List"),
            Key(input: "f", flags: .command, does: .find, title: "Find"),
        ]
        return keys
    }()

    private static let commands: [UIKeyCommand] = keys.map { key in
        let command = UIKeyCommand(
            title: key.title, action: #selector(CanvasKeys.keyed(_:)), input: key.input, modifierFlags: key.flags)
        command.wantsPriorityOverSystemBehavior = true
        return command
    }

    // MARK: The Edit menu's, on the characters selected while a text is typed in

    public override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        typealias Edits = UIResponderStandardEditActions
        switch action {
        case #selector(Edits.copy(_:)), #selector(Edits.cut(_:)):
            return typing.typing ? typing.selected != nil : clipping != nil
        case #selector(Edits.paste(_:)):
            return typing.typing ? UIPasteboard.general.hasStrings : clipping != nil
        case #selector(Edits.selectAll(_:)):
            return typing.typing
        case #selector(Edits.toggleBoldface(_:)), #selector(Edits.toggleItalics(_:)):
            return typing.typing && typing.from < typing.to
        case #selector(Edits.toggleUnderline(_:)):
            // A text is underlined where it links (PLAN 2.70), and nowhere else.
            return false
        default:
            return super.canPerformAction(action, withSender: sender)
        }
    }

    public override func copy(_ sender: Any?) {
        guard typing.typing else {
            clipping?(.copy)
            return
        }
        guard let selected = typing.selected else { return }
        UIPasteboard.general.string = selected
    }

    public override func cut(_ sender: Any?) {
        guard typing.typing else {
            clipping?(.cut)
            return
        }
        guard let selected = typing.selected else { return }
        UIPasteboard.general.string = selected
        typing.replace(typing.selection, with: "")
    }

    public override func paste(_ sender: Any?) {
        guard typing.typing else {
            clipping?(.paste)
            return
        }
        guard let text = UIPasteboard.general.string, !text.isEmpty else { return }
        typing.insert(text)
    }

    public override func selectAll(_ sender: Any?) {
        typing.selectAll()
    }

    /// The keyboard's shortcuts bar's B, as ⌘B.
    public override func toggleBoldface(_ sender: Any?) {
        typing.bold()
    }

    /// Its I, as ⌘I.
    public override func toggleItalics(_ sender: Any?) {
        typing.italic()
    }

    // MARK: Telling the input system

    /// What the input system sees: the text, the selection, and the composition.
    private struct Seen: Equatable {
        let text: String?
        let from: Int
        let to: Int
        let marked: NSRange?
    }

    private var now: Seen? {
        typing.typing ? Seen(text: typing.carets?.text, from: typing.from, to: typing.to, marked: typing.marked) : nil
    }

    /// Watch `Typing` for a change: once one comes, tell the input system of it.
    private func watch() {
        withObservationTracking {
            _ = (typing.node, typing.anchor, typing.head, typing.marked, typing.carets)
        } onChange: { [weak self] in
            Task { @MainActor [weak self] in
                self?.changed()
                self?.watch()
            }
        }
    }

    /// Tell the input system what changed since it last looked, where it did not change it; and
    /// once typing stops, give the keyboard up, to the canvas's own keys (PLAN 4.6).
    private func changed() {
        guard !asked else { return }
        if !typing.typing, isFirstResponder {
            _ = resignFirstResponder()
            presses.becomeFirstResponder()
        }
        let current = now
        guard current != seen else { return }
        let texts = current?.text != seen?.text || current?.marked != seen?.marked
        if texts { inputDelegate?.textWillChange(self) } else { inputDelegate?.selectionWillChange(self) }
        seen = current
        if texts { inputDelegate?.textDidChange(self) } else { inputDelegate?.selectionDidChange(self) }
    }

    /// Make a change the input system asked for: it knows of it already.
    private func inputting(_ change: () -> Void) {
        asked = true
        change()
        asked = false
        seen = now
    }

    // MARK: Where things are

    /// Points on the view to the canvas unit.
    private var scale: CGFloat { bounds.width / max(shown?.width ?? canvas.width, 1) }

    /// The canvas point at the view's top left.
    private var origin: CGPoint { shown?.origin ?? .zero }

    /// `box`, canvas units, on the view, points.
    fileprivate func onView(_ box: CGRect) -> CGRect {
        CGRect(
            x: (box.minX - origin.x) * scale, y: (box.minY - origin.y) * scale, width: box.width * scale,
            height: box.height * scale)
    }

    /// `point` on the view, points, on the canvas, canvas units.
    fileprivate func onCanvas(_ point: CGPoint) -> CGPoint {
        let s = max(scale, 1e-6)
        return CGPoint(x: point.x / s + origin.x, y: point.y / s + origin.y)
    }

    /// The line a caret at `offset` stands on, as the engine set it: where it starts and ends.
    fileprivate func lineRange(at offset: Int) -> NSRange? {
        guard let carets = typing.carets, !carets.lines.isEmpty else { return nil }
        let l = carets.line(of: offset)
        let start = carets.lines[l].start
        return NSRange(location: start, length: max(carets.end(ofLine: l) - start, 0))
    }

    /// The smallest box, axis-aligned, that holds `corners`.
    fileprivate static func box(around corners: [CGPoint]) -> CGRect {
        let (xs, ys) = (corners.map(\.x), corners.map(\.y))
        guard let x0 = xs.min(), let x1 = xs.max(), let y0 = ys.min(), let y1 = ys.max() else { return .null }
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }

    // MARK: VoiceOver, and what drives the keyboard in a test

    // While a text is typed in, the view stands for it: its words as typed, where they are drawn.

    public override var isAccessibilityElement: Bool {
        get { typing.typing }
        set {}
    }

    public override var accessibilityLabel: String? {
        get { typing.node == nil ? nil : "Editing text" }
        set {}
    }

    /// What a test finds it by while a text is typed in, as its label says: VoiceOver does not
    /// read it.
    public override var accessibilityIdentifier: String? {
        get { typing.node == nil ? nil : "typing" }
        set {}
    }

    public override var accessibilityValue: String? {
        get { typing.carets?.text }
        set {}
    }

    public override var accessibilityFrame: CGRect {
        get {
            guard let box = typing.box else { return .zero }
            return UIAccessibility.convertToScreenCoordinates(onView(Self.box(around: box.corners)), in: self)
        }
        set {}
    }
}

extension CanvasKeys: UITextInput {
    public var hasText: Bool { !(typing.carets?.text.isEmpty ?? true) }

    public func insertText(_ text: String) {
        guard typing.typing else { return }
        inputting {
            switch text {
            case "\n" where typing.marked == nil && typing.endsList:
                // In an empty item, the list ends there (ADR-0018); else a new paragraph, an item
                // like the one it leaves in a list.
                typing.list(kind: "none", done: "The list ends")
            case "\t" where typing.marked == nil:
                tab(back: false)
            default:
                typing.insert(text)
            }
        }
    }

    public func deleteBackward() {
        guard typing.typing else { return }
        inputting { typing.delete(backward: true, by: .character) }
    }

    /// What dictation heard, typed as the keys would type it.
    public func insertDictationResult(_ dictationResult: [UIDictationPhrase]) {
        insertText(dictationResult.map(\.text).joined())
    }

    public func text(in range: UITextRange) -> String? {
        guard let text = typing.carets?.text, let span = range as? Span else { return nil }
        let whole = text as NSString
        let start = min(span.from, whole.length)
        return whole.substring(with: NSRange(location: start, length: min(span.to, whole.length) - start))
    }

    public func replace(_ range: UITextRange, withText text: String) {
        guard typing.typing, let span = range as? Span else { return }
        inputting {
            // What autocorrection, an accent chosen, or Scribble's own edit replaces.
            typing.unmark()
            typing.replace(span.range, with: text)
        }
    }

    public var selectedTextRange: UITextRange? {
        get { typing.typing ? Span(typing.from, typing.to) : nil }
        set {
            guard typing.typing, let span = newValue as? Span else { return }
            inputting { typing.select(span.range) }
        }
    }

    public var markedTextRange: UITextRange? {
        guard typing.typing, let marked = typing.marked else { return nil }
        return Span(marked.location, NSMaxRange(marked))
    }

    public func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        guard typing.typing else { return }
        inputting { typing.mark(markedText ?? "", selected: selectedRange) }
    }

    public func unmarkText() {
        inputting { typing.unmark() }
    }

    public var beginningOfDocument: UITextPosition { Spot(0) }

    public var endOfDocument: UITextPosition { Spot(typing.carets?.length ?? 0) }

    public func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        guard let a = fromPosition as? Spot, let b = toPosition as? Spot else { return nil }
        return Span(a.offset, b.offset)
    }

    /// `offset` UTF-16 units on: what the input system counts in.
    public func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        guard let p = position as? Spot else { return nil }
        let at = p.offset + offset
        return at >= 0 && at <= typing.carets?.length ?? 0 ? Spot(at) : nil
    }

    /// Characters on as the reader sees them, left or right; lines up or down as the engine set
    /// them, nearest the caret's x.
    public func position(from position: UITextPosition, in direction: UITextLayoutDirection, offset: Int)
        -> UITextPosition?
    {
        guard let p = position as? Spot, let carets = typing.carets else { return nil }
        switch direction {
        case .left, .right:
            let on = (direction == .right) == (offset >= 0)
            var at = p.offset
            for _ in 0..<abs(offset) { at = on ? carets.after(at) : carets.before(at) }
            return Spot(at)
        case .up, .down:
            guard !carets.lines.isEmpty else { return p }
            let here = carets.caret(at: p.offset)
            let l = here.line + (direction == .up ? -offset : offset)
            if l < 0 { return Spot(0) }
            if l >= carets.lines.count { return Spot(carets.length) }
            return Spot(carets.offset(onLine: l, nearest: here.x))
        @unknown default:
            return nil
        }
    }

    public func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let (a, b) = ((position as? Spot)?.offset ?? 0, (other as? Spot)?.offset ?? 0)
        return a < b ? .orderedAscending : a > b ? .orderedDescending : .orderedSame
    }

    public func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        ((toPosition as? Spot)?.offset ?? 0) - ((from as? Spot)?.offset ?? 0)
    }

    public func position(within range: UITextRange, farthestIn direction: UITextLayoutDirection) -> UITextPosition? {
        guard let span = range as? Span else { return nil }
        return Spot(direction == .left || direction == .up ? span.from : span.to)
    }

    /// From `position` to its line's edge, left or right; to the text's, up or down.
    public func characterRange(byExtending position: UITextPosition, in direction: UITextLayoutDirection)
        -> UITextRange?
    {
        guard let p = position as? Spot, let carets = typing.carets else { return nil }
        let line = lineRange(at: p.offset)
        switch direction {
        case .left: return Span(line?.location ?? 0, p.offset)
        case .right: return Span(p.offset, line.map(NSMaxRange) ?? carets.length)
        case .up: return Span(0, p.offset)
        default: return Span(p.offset, carets.length)
        }
    }

    public func baseWritingDirection(for position: UITextPosition, in direction: UITextStorageDirection)
        -> NSWritingDirection
    {
        .natural
    }

    /// The engine sets each paragraph's direction from its characters.
    public func setBaseWritingDirection(_ writingDirection: NSWritingDirection, for range: UITextRange) {}

    /// Where the range's first line is drawn, or the caret's box where it covers nothing: where an
    /// input method puts its candidates.
    public func firstRect(for range: UITextRange) -> CGRect {
        guard let span = range as? Span else { return .zero }
        if let first = typing.covering(from: span.from, to: span.to).first { return onView(first) }
        return caretRect(for: Spot(span.from))
    }

    public func caretRect(for position: UITextPosition) -> CGRect {
        guard let p = position as? Spot, let box = typing.caretBox(at: p.offset) else { return .zero }
        var r = onView(box)
        r.size.width = max(r.width, 2)
        return r
    }

    public func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        guard let span = range as? Span else { return [] }
        let boxes = typing.covering(from: span.from, to: span.to)
        return boxes.indices.map { Covering(onView(boxes[$0]), starts: $0 == 0, ends: $0 == boxes.count - 1) }
    }

    public func closestPosition(to point: CGPoint) -> UITextPosition? {
        guard typing.typing else { return nil }
        return Spot(typing.offset(near: onCanvas(point)))
    }

    public func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        guard typing.typing, let span = range as? Span else { return nil }
        return Spot(min(max(typing.offset(near: onCanvas(point)), span.from), span.to))
    }

    public func characterRange(at point: CGPoint) -> UITextRange? {
        guard let found = typing.character(at: onCanvas(point)) else { return nil }
        return Span(found.location, NSMaxRange(found))
    }
}

extension CanvasKeys: @preconcurrency UIIndirectScribbleInteractionDelegate {
    /// The state shown's texts the Pencil writes in, by node, near `rect` on the view.
    public func indirectScribbleInteraction(
        _ interaction: any UIInteraction, requestElementsIn rect: CGRect, completion: @escaping ([String]) -> Void
    ) {
        completion(typing.writable().filter { onView(Self.box(around: $0.corners)).intersects(rect) }.map(\.node))
    }

    public func indirectScribbleInteraction(_ interaction: any UIInteraction, isElementFocused elementIdentifier: String)
        -> Bool
    {
        typing.node == elementIdentifier && isFirstResponder
    }

    public func indirectScribbleInteraction(_ interaction: any UIInteraction, frameForElement elementIdentifier: String)
        -> CGRect
    {
        typing.writable().first { $0.node == elementIdentifier }.map { onView(Self.box(around: $0.corners)) } ?? .zero
    }

    /// Writing on a text begins typing in it, at the caret nearest where the Pencil began, as a
    /// double tap does (PLAN 4.3); the text is the input system's until typing stops.
    public func indirectScribbleInteraction(
        _ interaction: any UIInteraction, focusElementIfNeeded elementIdentifier: String,
        referencePoint focusReferencePoint: CGPoint, completion: @escaping ((UIResponder & UITextInput)?) -> Void
    ) {
        if typing.node != elementIdentifier, let state = typing.editor.shown {
            typing.enter(elementIdentifier, in: state, at: onCanvas(focusReferencePoint))
        }
        guard typing.node == elementIdentifier else { return completion(nil) }
        if !isFirstResponder { becomeFirstResponder() }
        completion(self)
    }
}

/// A place between two characters of the text typed in, UTF-16, as `UITextInput` names one.
final class Spot: UITextPosition {
    let offset: Int

    init(_ offset: Int) {
        self.offset = offset
        super.init()
    }
}

/// A run of the text typed in, UTF-16, as `UITextInput` names one.
final class Span: UITextRange {
    let from: Int
    let to: Int

    init(_ a: Int, _ b: Int) {
        from = max(min(a, b), 0)
        to = max(a, b, 0)
        super.init()
    }

    var range: NSRange { NSRange(location: from, length: to - from) }

    override var start: UITextPosition { Spot(from) }
    override var end: UITextPosition { Spot(to) }
    override var isEmpty: Bool { from == to }
}

/// A stretch of a selection on one line, on the view, as `UITextInput` names one.
final class Covering: UITextSelectionRect {
    private let box: CGRect
    private let starts: Bool
    private let ends: Bool

    init(_ box: CGRect, starts: Bool, ends: Bool) {
        self.box = box
        self.starts = starts
        self.ends = ends
        super.init()
    }

    override var rect: CGRect { box }
    override var writingDirection: NSWritingDirection { .natural }
    override var containsStart: Bool { starts }
    override var containsEnd: Bool { ends }
    override var isVertical: Bool { false }
}

/// Where the text's units begin and end, for the input system: its lines as the engine set them;
/// its characters, words, sentences, and paragraphs as the system finds them in its characters.
final class Boundaries: UITextInputStringTokenizer {
    private unowned let keys: CanvasKeys

    init(keys: CanvasKeys) {
        self.keys = keys
        super.init(textInput: keys)
    }

    /// Whether `direction` goes on through the text: forward, right, or down.
    private func on(_ direction: UITextDirection) -> Bool {
        [UITextStorageDirection.forward.rawValue, UITextLayoutDirection.right.rawValue, UITextLayoutDirection.down.rawValue]
            .contains(direction.rawValue)
    }

    /// Where `position` stands, and the line it stands on, where the unit is a line.
    private func stands(_ position: UITextPosition, _ granularity: UITextGranularity) -> (at: Int, line: NSRange)? {
        guard granularity == .line, let p = position as? Spot, let line = keys.lineRange(at: p.offset) else {
            return nil
        }
        return (p.offset, line)
    }

    override func rangeEnclosingPosition(
        _ position: UITextPosition, with granularity: UITextGranularity, inDirection direction: UITextDirection
    ) -> UITextRange? {
        guard let found = stands(position, granularity) else {
            return super.rangeEnclosingPosition(position, with: granularity, inDirection: direction)
        }
        return Span(found.line.location, NSMaxRange(found.line))
    }

    override func isPosition(
        _ position: UITextPosition, atBoundary granularity: UITextGranularity, inDirection direction: UITextDirection
    ) -> Bool {
        guard let found = stands(position, granularity) else {
            return super.isPosition(position, atBoundary: granularity, inDirection: direction)
        }
        return found.at == (on(direction) ? NSMaxRange(found.line) : found.line.location)
    }

    override func position(
        from position: UITextPosition, toBoundary granularity: UITextGranularity, inDirection direction: UITextDirection
    ) -> UITextPosition? {
        guard let found = stands(position, granularity) else {
            return super.position(from: position, toBoundary: granularity, inDirection: direction)
        }
        return Spot(on(direction) ? NSMaxRange(found.line) : found.line.location)
    }

    override func isPosition(
        _ position: UITextPosition, withinTextUnit granularity: UITextGranularity, inDirection direction: UITextDirection
    ) -> Bool {
        guard let found = stands(position, granularity) else {
            return super.isPosition(position, withinTextUnit: granularity, inDirection: direction)
        }
        return on(direction) ? found.at < NSMaxRange(found.line) : found.at > found.line.location
    }
}

/// `CanvasKeys` in SwiftUI, under the canvas: the keyboard's and the Pencil's while a text is
/// typed in, and the canvas's own keys while none is (PLAN 4.6).
public struct CanvasKeysHost: UIViewRepresentable {
    public let typing: Typing
    /// The canvas, in canvas units.
    public let canvas: CGSize
    /// The part of it shown, where it is zoomed in.
    public let shown: CGRect?
    public let clipping: @MainActor (Clipping) -> Void
    public let finding: (@MainActor () -> Void)?
    /// A hardware keyboard's key on the canvas while no text is typed in: whether it took it.
    public let pressed: (@MainActor (UIKey) -> Bool)?
    /// Escape or Return on the canvas while no text is typed in, which the system hands it as a
    /// key command rather than a press: the key's input and its flags, and whether it took it.
    public let commanded: (@MainActor (String, UIKeyModifierFlags) -> Bool)?
    /// ⌘A on the canvas while no text is typed in: whether it selected anything.
    public let selectingAll: (@MainActor () -> Bool)?

    public init(
        typing: Typing, canvas: CGSize, shown: CGRect? = nil, clipping: @escaping @MainActor (Clipping) -> Void,
        finding: (@MainActor () -> Void)? = nil, pressed: (@MainActor (UIKey) -> Bool)? = nil,
        commanded: (@MainActor (String, UIKeyModifierFlags) -> Bool)? = nil,
        selectingAll: (@MainActor () -> Bool)? = nil
    ) {
        self.typing = typing
        self.canvas = canvas
        self.shown = shown
        self.clipping = clipping
        self.finding = finding
        self.pressed = pressed
        self.commanded = commanded
        self.selectingAll = selectingAll
    }

    public func makeUIView(context: Context) -> CanvasKeys {
        let view = CanvasKeys(typing: typing)
        updateUIView(view, context: context)
        return view
    }

    public func updateUIView(_ view: CanvasKeys, context: Context) {
        view.canvas = canvas
        view.shown = shown
        view.clipping = clipping
        view.finding = finding
        view.presses.pressed = pressed
        view.presses.commanded = commanded
        view.presses.clipping = clipping
        view.presses.selectingAll = selectingAll
    }
}

/// The canvas's own keys on the iPad while no text is typed in (PLAN 4.6), as the Mac's
/// `CanvasKeys` takes them then (PLAN 3.17): a hardware keyboard's Tab, Return, Escape, the arrows,
/// Space, the brackets, +, and Delete, each handed to the canvas first (`pressed`), which says
/// whether it took it. One it does not take goes on, so Tab past either end leaves the slide, and
/// the app's commands keep their keys. Escape and Return reach it as no press, the iPad's UI tests'
/// key log says: the system answers them first. So they are key commands of the canvas's own,
/// ahead of the system's (`commanded`), which a sheet or a popover over the canvas leaves it. The
/// Edit menu's Copy, Cut, Paste, and Select All are the canvas's (`clipping`, `selectingAll`). It
/// is no text input, so no software keyboard comes with it, and it takes no touch.
///
/// A command run by its key can take the keyboard from it with nothing taking it in its place:
/// ⌘Z's Undo ends editing in the window first. The keyboard comes back once the command has run,
/// unless the canvas let it go on purpose (Tab past either end) or something else has it now: a
/// field, a list, a sheet or a popover over the canvas, typing, or another window.
@MainActor
public final class CanvasPresses: UIView {
    public var pressed: (@MainActor (UIKey) -> Bool)?
    /// Escape or Return, by its input and flags: whether the canvas took it.
    public var commanded: (@MainActor (String, UIKeyModifierFlags) -> Bool)?
    public var clipping: (@MainActor (Clipping) -> Void)?
    public var selectingAll: (@MainActor () -> Bool)?
    /// The presses the canvas took: how each goes on and ends is its too.
    private var taken: Set<UIPress> = []
    /// Whether the canvas let the keyboard go: Tab past either end of the slide.
    private var lettingGo = false
    /// The window's becoming key and an undo's running, logged beside the keys (`keysLog`).
    private var watching: [NSObjectProtocol] = []

    public override var canBecomeFirstResponder: Bool { true }

    /// Escape and Return, and Return with Shift or Option, ahead of the system's own behavior for
    /// them (`wantsPriorityOverSystemBehavior`).
    public override var keyCommands: [UIKeyCommand]? {
        commanded == nil ? nil : Self.commands
    }

    private static let commands: [UIKeyCommand] = {
        let keys: [(String, UIKeyModifierFlags)] = [
            (UIKeyCommand.inputEscape, []),
            ("\r", []), ("\r", .shift), ("\r", .alternate), ("\r", [.shift, .alternate]),
        ]
        return keys.map { input, flags in
            let command = UIKeyCommand(input: input, modifierFlags: flags, action: #selector(CanvasPresses.keyed(_:)))
            command.wantsPriorityOverSystemBehavior = true
            return command
        }
    }()

    @objc private func keyed(_ command: UIKeyCommand) {
        guard let input = command.input else { return }
        let took = commanded?(input, command.modifierFlags) == true
        let key = input == UIKeyCommand.inputEscape ? "Escape" : "Return"
        let flags = command.modifierFlags.rawValue
        keysLog.debug("command \(key, privacy: .public) flags \(flags): \(took ? "taken" : "not taken", privacy: .public)")
    }

    public override func didMoveToWindow() {
        super.didMoveToWindow()
        let center = NotificationCenter.default
        for watched in watching { center.removeObserver(watched) }
        watching = []
        guard window != nil else { return }
        let said: [(Notification.Name, String)] = [
            (UIWindow.didBecomeKeyNotification, "became key"), (UIWindow.didResignKeyNotification, "resigned key"),
            (Notification.Name("NSUndoManagerWillUndoChangeNotification"), "will undo"),
            (Notification.Name("NSUndoManagerDidUndoChangeNotification"), "did undo"),
        ]
        for (name, what) in said {
            watching.append(
                center.addObserver(forName: name, object: nil, queue: .main) { note in
                    let which = note.object.map { String(describing: type(of: $0)) } ?? "none"
                    keysLog.debug("\(which, privacy: .public) \(what, privacy: .public)")
                })
        }
        // A field elsewhere in the window that stops taking the keys, as Return in the inspector's
        // does, gives them back to the canvas where nothing else has them: on the iPad nothing
        // holds the keyboard after it, and ⌘Z and the canvas's keys would go nowhere (PLAN 3.28).
        for ended in [UITextField.textDidEndEditingNotification, UITextView.textDidEndEditingNotification] {
            watching.append(
                center.addObserver(forName: ended, object: nil, queue: .main) { [weak self] _ in
                    MainActor.assumeIsolated { self?.takeBackSoon() }
                })
        }
        // Each key as the keyboard itself reports it, below UIKit's presses and key commands: what
        // reached the app at all; from a keyboard here now, or one that connects.
        Self.hearKeyboard()
        watching.append(
            center.addObserver(forName: .GCKeyboardDidConnect, object: nil, queue: .main) { _ in
                MainActor.assumeIsolated { Self.hearKeyboard() }
            })
    }

    /// Each key the keyboard reports, logged as it goes down and comes up (`keysLog`).
    private static func hearKeyboard() {
        GCKeyboard.coalesced?.keyboardInput?.keyChangedHandler = { _, _, code, down in
            keysLog.debug("keyboard \(code.rawValue) \(down ? "down" : "up", privacy: .public)")
        }
    }

    /// Touches go through to the canvas over it.
    public override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? { nil }

    public override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var rest = Set<UIPress>()
        for press in presses {
            let code = press.key?.keyCode.rawValue ?? -1
            let flags = press.key?.modifierFlags.rawValue ?? 0
            if let key = press.key, pressed?(key) == true {
                keysLog.debug("press \(code) flags \(flags): taken")
                taken.insert(press)
                lettingGo = false
            } else {
                keysLog.debug("press \(code) flags \(flags): passed on")
                if press.key?.keyCode == .keyboardTab { lettingGo = true }
                rest.insert(press)
            }
        }
        if !rest.isEmpty { super.pressesBegan(rest, with: event) }
    }

    @discardableResult
    public override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        keysLog.debug("canvas keys became first responder: \(became)")
        if became { lettingGo = false }
        return became
    }

    @discardableResult
    public override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        let going = lettingGo
        keysLog.debug("canvas keys resigned: \(resigned), let go: \(going)")
        if resigned, !lettingGo {
            Task { @MainActor [weak self] in self?.takeBack() }
        }
        return resigned
    }

    /// The keyboard taken back once the event that let it go has run (`takeBack`): a field
    /// elsewhere stopped taking it.
    private func takeBackSoon() {
        keysLog.debug("a field stopped taking the keys")
        Task { @MainActor [weak self] in self?.takeBack() }
    }

    /// The keyboard taken back, once what took it has run, where nothing else has it.
    private func takeBack() {
        let (hasWindow, key, first, over) = (window != nil, window?.isKeyWindow == true, isFirstResponder, covered)
        let typing = (superview as? CanvasKeys)?.typing.typing == true
        guard let window, key, !first, !over, !typing else {
            keysLog.debug(
                "take back: no; window \(hasWindow), key \(key), first \(first), covered \(over), typing \(typing)")
            return
        }
        // What has the keys now, if anything: none but what holds the canvas lets it take them.
        if let holder = CanvasKeys.responder(in: window), holder is UIKeyInput || !isDescendant(of: holder) {
            let name = String(describing: type(of: holder))
            keysLog.debug("take back: no; \(name, privacy: .public) has the keys")
            return
        }
        let became = becomeFirstResponder()
        keysLog.debug("take back: \(became)")
    }

    /// Whether a sheet, a popover, or an alert shows over the canvas.
    private var covered: Bool {
        var responder: UIResponder? = next
        while let at = responder {
            if let controller = at as? UIViewController, let shown = controller.presentedViewController,
                !(shown.viewIfLoaded.map { isDescendant(of: $0) } ?? false)
            {
                return true
            }
            responder = at.next
        }
        return false
    }

    public override func pressesChanged(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.subtracting(taken)
        if !rest.isEmpty { super.pressesChanged(rest, with: event) }
    }

    public override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.subtracting(taken)
        taken.subtract(presses)
        if !rest.isEmpty { super.pressesEnded(rest, with: event) }
    }

    public override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.subtracting(taken)
        taken.subtract(presses)
        if !rest.isEmpty { super.pressesCancelled(rest, with: event) }
    }

    // MARK: The Edit menu's, on the canvas

    public override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        typealias Edits = UIResponderStandardEditActions
        switch action {
        case #selector(Edits.copy(_:)), #selector(Edits.cut(_:)), #selector(Edits.paste(_:)):
            return clipping != nil
        case #selector(Edits.selectAll(_:)):
            return selectingAll != nil
        case #selector(CanvasPresses.keyed(_:)):
            // A sheet or a popover over the canvas keeps Escape and Return for itself.
            return commanded != nil && !covered
        default:
            return super.canPerformAction(action, withSender: sender)
        }
    }

    public override func copy(_ sender: Any?) {
        clipping?(.copy)
    }

    public override func cut(_ sender: Any?) {
        clipping?(.cut)
    }

    public override func paste(_ sender: Any?) {
        clipping?(.paste)
    }

    public override func selectAll(_ sender: Any?) {
        _ = selectingAll?()
    }
}
#endif
