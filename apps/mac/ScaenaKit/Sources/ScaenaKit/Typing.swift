import AppKit
import Foundation
import Observation

/// Text typed in place on the canvas (PLAN 3.9), as the browser's canvas types it (PLAN 2.32,
/// `web/src/typing.ts`): a double click puts a caret in a text where the engine says the character
/// is, and what is typed there makes `replace_text` patches, by the user, written where the text
/// lives, or kept to the state with Option (`fork`). Nothing here lays out: the caret, the
/// selection, and where a caret goes up or down a line come from the engine's carets. Keys and an
/// input method's composition come in through `TypingView`, which the canvas holds.
///
/// - Each change is one `replace_text`, made at once; a burst of typing, its keys less than a
///   second apart with nothing else edited between, is one step to undo (`edited`).
/// - What an input method composes is typed in place, so the engine sets it as it will read, and
///   stays where it is when the composition commits.
/// - ⌘B and ⌘I give the characters selected a look (PLAN 2.38, 2.40): one `style_text`, one step
///   to undo.
@MainActor @Observable
public final class Typing {
    public let editor: DeckEditor
    /// The text typed in, and the state it is typed in.
    public private(set) var node: String?
    public private(set) var state: String?
    /// Whether what is typed is kept to that state (`fork`).
    public private(set) var fork = false
    /// Where a caret stands in it, as the engine last laid it out.
    public private(set) var carets: Carets?
    /// The selection, UTF-16: where it is anchored, and its head, where the caret is.
    public private(set) var anchor = 0
    public private(set) var head = 0
    /// What an input method is composing, UTF-16, typed in place until it commits.
    public private(set) var marked: NSRange?
    /// The text's box at rest, as the engine says it stands (ADR-0013): where a press types in it.
    public private(set) var box: NodeBox?
    /// What the status line says of typing: where it goes, or why something was not typed.
    public private(set) var told: String?
    /// The window's undo, given the source each change replaced and whether it joins the burst of
    /// typing before it.
    @ObservationIgnored public var edited: ((String, Bool) -> Void)?
    /// Asked to take the keyboard: a text entered, or pressed in.
    @ObservationIgnored public var focus: (@MainActor () -> Void)?
    /// Told when a composition ends by something other than the input method (a press, a move,
    /// typing left), so that it stops composing too.
    @ObservationIgnored public var discarded: (@MainActor () -> Void)?
    /// How long a pause between two keys ends a burst of typing.
    @ObservationIgnored public var burst = Duration.seconds(1)

    /// The line a caret stands on, where it could stand on two: a line's end or the next's start.
    @ObservationIgnored private var on: Int?
    /// Where up and down aim, canvas x: the caret's x when they began.
    @ObservationIgnored private var goal: Double?
    /// When the last change was typed, and the revision of the deck it left: a key typed within a
    /// burst of it, nothing else edited since, joins it.
    @ObservationIgnored private var last: (at: ContinuousClock.Instant, revision: Int)?
    /// Where typing goes, as the status says it once a change is typed.
    @ObservationIgnored private var whereTold: String?

    public init(editor: DeckEditor) {
        self.editor = editor
    }

    /// Whether a text is typed in.
    public var typing: Bool { node != nil }
    /// The selection's start and end, UTF-16.
    public var from: Int { min(anchor, head) }
    public var to: Int { max(anchor, head) }
    public var selection: NSRange { NSRange(location: from, length: to - from) }

    /// The characters selected, if any are.
    public var selected: String? {
        guard let carets, from < to else { return nil }
        return (carets.text as NSString).substring(with: selection)
    }

    /// The map `[a, b, c, d, e, f]` from the text's box as laid out to where it is drawn (its
    /// `transform`): what draws the caret, and reads a point back into the text.
    public var map: [Double]? { box?.transform }

    /// Begin typing in `node`, as `state` shows it, at the caret nearest `point` (canvas units,
    /// where it is drawn), or at its end without one; `fork` keeps what is typed to that state.
    /// False for a node that is no text there.
    @discardableResult
    public func enter(_ node: String, in state: String, at point: CGPoint?, fork: Bool = false) -> Bool {
        guard let found = try? editor.session.carets(state: state, node: node) else { return false }
        commit()
        self.node = node
        self.state = state
        self.fork = fork
        carets = found
        box = try? editor.session.boxes(state: state).first { $0.node == node }
        last = nil
        if let point {
            let (offset, line) = found.caret(near: back(map, point))
            put(offset, line: line)
        } else {
            put(found.length)
        }
        tellWhere()
        focus?()
        return true
    }

    /// Stop typing; the node stays selected.
    public func leave() {
        node = nil
        state = nil
        carets = nil
        box = nil
        commit()
        told = nil
        whereTold = nil
        on = nil
        goal = nil
        last = nil
    }

    /// Whether `point`, canvas units where it is drawn, is in the text typed in: in its box, or
    /// within `slop` of it, as the browser's canvas reads a press (PLAN 2.32).
    public func holds(_ point: CGPoint, slop: Double = 0) -> Bool {
        guard typing, let rect = box?.rect, rect.count == 4 else { return false }
        let p = back(map, point)
        let (x, y) = (Double(p.x), Double(p.y))
        return x >= rect[0] - slop && x <= rect[0] + rect[2] + slop && y >= rect[1] - slop && y <= rect[1] + rect[3] + slop
    }

    /// Read the text again from the deck, where something else changed it (an undo, the source
    /// typed, a gesture): the caret kept where it still fits. Typing stops where the state shown
    /// is another, or the node is no text in it now.
    public func sync(shown: String?) {
        guard let node, let state else { return }
        guard shown == state else { return leave() }
        // The text's own change: read already.
        if let last, last.revision == editor.revision { return }
        guard let found = try? editor.session.carets(state: state, node: node) else { return leave() }
        box = try? editor.session.boxes(state: state).first { $0.node == node }
        if found != carets {
            carets = found
            anchor = min(anchor, found.length)
            head = min(head, found.length)
            commit()
            on = nil
            goal = nil
        }
        // What is typed next is a step to undo of its own.
        self.last = nil
    }

    // MARK: What is typed

    /// Type `text` where the selection is, or the composition: one `replace_text`.
    public func insert(_ text: String) {
        let range = marked ?? selection
        marked = nil
        replace(range, with: text)
    }

    /// An input method's composition, `text`, typed in place of the selection or the composition
    /// before it, `selected` its selection inside it.
    public func mark(_ text: String, selected: NSRange) {
        let range = marked ?? selection
        guard !text.isEmpty else {
            marked = nil
            if range.length > 0 { replace(range, with: "") }
            return
        }
        guard replace(range, with: text) else { return }
        let length = text.utf16.count
        marked = NSRange(location: range.location, length: length)
        let at = range.location + min(selected.location, length)
        anchor = at
        head = min(at + selected.length, range.location + length)
    }

    /// The composition commits as it stands.
    public func unmark() {
        marked = nil
    }

    /// Select `range`, UTF-16, as far as the text goes: what an input method recomposes.
    public func select(_ range: NSRange) {
        guard let carets else { return }
        commit()
        let length = carets.length
        anchor = min(max(range.location, 0), length)
        head = min(anchor + max(range.length, 0), length)
        on = nil
        goal = nil
    }

    /// How far a delete or a move goes.
    public enum Unit: Sendable {
        case character, word, line
    }

    /// Delete the selection, or else what `unit` holds before the caret (`backward`) or after it.
    public func delete(backward: Bool, by unit: Unit) {
        guard let carets else { return }
        guard from == to else {
            replace(selection, with: "")
            return
        }
        let other: Int
        switch (unit, backward) {
        case (.character, true): other = carets.before(head)
        case (.character, false): other = carets.after(head)
        case (.word, true): other = carets.wordBefore(head)
        case (.word, false): other = carets.wordAfter(head)
        case (.line, true): other = carets.lines.isEmpty ? 0 : carets.lines[carets.line(of: head, on: on)].start
        case (.line, false): other = carets.lines.isEmpty ? carets.length : carets.end(ofLine: carets.line(of: head, on: on))
        }
        guard other != head else { return }
        replace(NSRange(location: min(other, head), length: abs(other - head)), with: "")
    }

    /// Replace `range` (UTF-16) with `text`: one `replace_text`, the caret after it. False where it
    /// was not typed, the status saying why.
    @discardableResult
    public func replace(_ range: NSRange, with text: String) -> Bool {
        guard let node, let state, let carets, range.location != NSNotFound else { return false }
        // Kept to the text. Not `NSIntersectionRange`: it makes a caret, an empty range, `{0, 0}`.
        let start = min(max(range.location, 0), carets.length)
        let range = NSRange(location: start, length: min(max(range.length, 0), carets.length - start))
        // What reads so already (a composition committed as it was typed) only moves the caret.
        if (carets.text as NSString).substring(with: range) == text {
            put(range.location + text.utf16.count)
            return true
        }
        var op: [String: JSONValue] = [
            "op": "replace_text", "node": .string(node), "state": .string(state),
            "from": .number(Double(carets.scalars(range.location))),
            "to": .number(Double(carets.scalars(NSMaxRange(range)))),
            "text": .string(text),
        ]
        if fork { op["fork"] = true }
        let now = ContinuousClock.now
        let joins = last.map { now - $0.at < burst && $0.revision == editor.revision } ?? false
        do {
            let before = try editor.typed([.object(op)])
            last = (now, editor.revision)
            told = whereTold
            edited?(before, joins)
        } catch {
            told = "not typed: \(error)"
            return false
        }
        guard reread() else { return false }
        put(min(range.location + text.utf16.count, self.carets?.length ?? 0))
        return true
    }

    // MARK: Where the caret goes

    /// How far a move goes.
    public enum Step: Sendable {
        /// A character back (-1) or on (1).
        case character(Int)
        /// To the start of the word before, or the end of the word after.
        case word(Int)
        /// Up (-1) or down (1) a line, as the text is set.
        case line(Int)
        /// To the line's start, or its end.
        case lineEdge(start: Bool)
        /// To the text's start, or its end.
        case text(start: Bool)
    }

    /// Move the caret by `step`; with `extend`, the selection's head moves and its anchor stays.
    public func move(_ step: Step, extend: Bool = false) {
        guard let carets, !carets.lines.isEmpty else { return }
        var next: Int
        var line: Int?
        switch step {
        case .character(let by):
            next = !extend && from != to ? (by < 0 ? from : to) : (by < 0 ? carets.before(head) : carets.after(head))
            goal = nil
        case .word(let by):
            next = by < 0 ? carets.wordBefore(head) : carets.wordAfter(head)
            goal = nil
        case .line(let by):
            let here = carets.caret(at: head, on: on)
            let aim = goal ?? here.x
            goal = aim
            let l = here.line + by
            if l < 0 {
                (next, line) = (0, 0)
            } else if l >= carets.lines.count {
                (next, line) = (carets.length, carets.lines.count - 1)
            } else {
                (next, line) = (carets.offset(onLine: l, nearest: aim), l)
            }
        case .lineEdge(let start):
            let l = carets.line(of: head, on: on)
            next = start ? carets.lines[l].start : carets.end(ofLine: l)
            line = l
            goal = nil
        case .text(let start):
            next = start ? 0 : carets.length
            goal = nil
        }
        head = next
        if !extend { anchor = next }
        on = line
        commit()
    }

    /// Select every character.
    public func selectAll() {
        guard let carets else { return }
        commit()
        anchor = 0
        head = carets.length
        on = nil
        goal = nil
    }

    /// A press at `point` (canvas units) in the text: the caret there, or with `extend` the
    /// selection's head; a second click selects the word there, a third its paragraph.
    public func press(at point: CGPoint, clicks: Int = 1, extend: Bool = false) {
        guard let carets else { return }
        commit()
        focus?()
        let (offset, line) = carets.caret(near: back(map, point))
        switch clicks {
        case 2:
            let word = carets.word(at: offset)
            (anchor, head) = (word.location, NSMaxRange(word))
        case 3...:
            let paragraph = carets.paragraph(at: offset)
            (anchor, head) = (paragraph.location, NSMaxRange(paragraph))
        default:
            head = offset
            if !extend { anchor = offset }
        }
        on = line
        goal = nil
    }

    /// A press dragged to `point` (canvas units): the selection's head there.
    public func drag(to point: CGPoint) {
        guard let carets else { return }
        let (offset, line) = carets.caret(near: back(map, point))
        head = offset
        on = line
    }

    /// The offset nearest `point`, canvas units: what an input method asks of a point.
    public func offset(near point: CGPoint) -> Int {
        carets?.caret(near: back(map, point)).offset ?? 0
    }

    /// The caret at `offset` as a line box, canvas units, where its text is drawn: the corners
    /// through its map, clockwise from the top left.
    public func caretCorners(at offset: Int) -> [CGPoint]? {
        guard let r = carets?.box(at: offset, on: on) else { return nil }
        return [CGPoint(x: r.minX, y: r.minY), CGPoint(x: r.minX, y: r.maxY)].map { through(map, $0) }
    }

    /// The caret where it stands now, as `caretCorners` gives it: its top and its bottom.
    public var caret: [CGPoint]? { caretCorners(at: head) }

    /// What the selection covers, each rectangle's corners where the text is drawn.
    public var covered: [[CGPoint]] {
        guard let carets, from < to else { return [] }
        return carets.covered(from: from, to: to).map(corners)
    }

    /// What an input method composes, underlined: each rectangle's corners where it is drawn.
    public var composing: [[CGPoint]] {
        guard let carets, let marked, marked.length > 0 else { return [] }
        return carets.covered(from: marked.location, to: NSMaxRange(marked)).map(corners)
    }

    /// The caret's line box at `offset` on the canvas, axis-aligned: where an input method's window goes.
    public func caretBox(at offset: Int) -> CGRect? {
        guard let r = carets?.box(at: offset, on: on) else { return nil }
        let points = corners(r)
        let xs = points.map(\.x)
        let ys = points.map(\.y)
        guard let x0 = xs.min(), let x1 = xs.max(), let y0 = ys.min(), let y1 = ys.max() else { return nil }
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }

    // MARK: A look for the characters selected

    /// ⌘B: the characters selected bold, or, all bold already, not (PLAN 2.38), as the engine reads
    /// the weight it sets each in.
    public func bold() {
        guard let node, let state, let carets, from < to else { return say("select characters to make them bold") }
        do {
            let look = try editor.session.bolding(
                state: state, node: node, from: carets.scalars(from), to: carets.scalars(to))
            style(look, said: look["style/weight"]?.number == 700 ? "bold" : "not bold")
        } catch {
            say("not bold: \(error)")
        }
    }

    /// ⌘I: the characters selected in italic, or, all italic already, not (PLAN 2.40), as each
    /// asks for it: a family without an italic face sets them upright all the same (W231).
    public func italic() {
        guard let node, let state, let carets, from < to else {
            return say("select characters to set them in italic")
        }
        do {
            let look = try editor.session.italicizing(
                state: state, node: node, from: carets.scalars(from), to: carets.scalars(to))
            style(look, said: look["style/italic"]?.bool == true ? "italic" : "upright")
        } catch {
            say("not italic: \(error)")
        }
    }

    /// Give the characters selected `look`: one `style_text`, one step to undo; the selection stays.
    private func style(_ look: JSONValue, said: String) {
        guard let node, let state, let carets, from < to else { return }
        var op: [String: JSONValue] = [
            "op": "style_text", "node": .string(node), "state": .string(state),
            "from": .number(Double(carets.scalars(from))), "to": .number(Double(carets.scalars(to))),
            "look": look,
        ]
        if fork { op["fork"] = true }
        let (keep, length) = (selection, carets.length)
        do {
            let before = try editor.typed([.object(op)])
            last = nil
            edited?(before, false)
        } catch {
            return say("no look given: \(error)")
        }
        guard reread(), let found = self.carets else { return }
        // A look that changes the characters (a quote's figure, PLAN 2.72) keeps them selected.
        anchor = min(keep.location, found.length)
        head = max(anchor, min(NSMaxRange(keep) + found.length - length, found.length))
        say("\(node), characters \(carets.scalars(keep.location) + 1)–\(carets.scalars(NSMaxRange(keep))): \(said)")
    }

    // MARK: Inside

    /// Read the text and its box again after a change typed: false, typing left, where the node
    /// is no text now.
    private func reread() -> Bool {
        guard let node, let state, let found = try? editor.session.carets(state: state, node: node) else {
            leave()
            return false
        }
        carets = found
        box = try? editor.session.boxes(state: state).first { $0.node == node }
        return true
    }

    /// End a composition the input method did not end: what it composed stays as typed.
    private func commit() {
        guard marked != nil else { return }
        marked = nil
        discarded?()
    }

    /// The caret at `offset`, on `line` where it could stand on two.
    private func put(_ offset: Int, line: Int? = nil) {
        anchor = offset
        head = offset
        on = line
        goal = nil
    }

    /// `rect`, canvas units as the text is laid out, as corners where it is drawn.
    private func corners(_ rect: CGRect) -> [CGPoint] {
        [
            CGPoint(x: rect.minX, y: rect.minY), CGPoint(x: rect.maxX, y: rect.minY),
            CGPoint(x: rect.maxX, y: rect.maxY), CGPoint(x: rect.minX, y: rect.maxY),
        ].map { through(map, $0) }
    }

    private func say(_ words: String) {
        told = words
    }

    /// Say where typing goes: the states it reaches, as the browser's status says it.
    private func tellWhere() {
        guard let node, let state else { return }
        var op: [String: JSONValue] = [
            "op": "replace_text", "node": .string(node), "state": .string(state), "from": 0, "to": 0, "text": "·",
        ]
        if fork { op["fork"] = true }
        let states = (try? editor.session.reach([.object(op)])) ?? [state]
        let n = states.count
        let reaches = fork ? "kept to \(state)" : n == 1 && states.first == state ? "in this state" : "in \(n) states"
        let keep = fork || n < 2 ? "" : "; Option and a double click keep it to \(state)"
        whereTold = "typing in \(node) · \(reaches) · Escape leaves it\(keep)"
        told = whereTold
    }
}

extension ScaenaSession {
    /// Where a caret stands in `node`'s text as `state` shows it at rest (PLAN 2.32): none for a
    /// node that is no text there.
    public func carets(state: String, node: String) throws -> Carets? {
        try call("carets", ["state": .string(state), "node": .string(node)])
    }

    /// Make `ops` typed in place, by the user at `date` (PLAN 3.9): validated, not linted.
    /// Whether the deck changed.
    public func typed(_ ops: [JSONValue], at date: Date = Date()) throws -> Bool {
        try call("typed", ["ops": .array(ops), "at": .string(date.formatted(.iso8601))])
    }

    /// The look ⌘B gives `node`'s characters `from` to `to` (Unicode scalar values) in `state`
    /// (PLAN 2.38): `style_text`'s `look`.
    public func bolding(state: String, node: String, from: Int, to: Int) throws -> JSONValue {
        try call("bolding", ["state": .string(state), "node": .string(node), "from": .number(Double(from)), "to": .number(Double(to))])
    }

    /// The look ⌘I gives them (PLAN 2.40).
    public func italicizing(state: String, node: String, from: Int, to: Int) throws -> JSONValue {
        try call(
            "italicizing", ["state": .string(state), "node": .string(node), "from": .number(Double(from)), "to": .number(Double(to))])
    }
}
