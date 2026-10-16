import CoreGraphics
import Dispatch
import Foundation
import Observation
import Synchronization

/// A deck open for editing, as the browser's editor keeps one (PLAN 2.3, 3.4): the session, the
/// deck as `.scn` as the source pane shows it, and what compiling and lint found.
///
/// The source is what the deck is made from. A patch (a gesture, a choice in the inspector) makes
/// the deck's source again, a finding's fix rewrites it, and a source typed is compiled as it
/// stands: each is compiled, becomes the deck where it validates, and lints the state shown at
/// once; `lintEvery` lints every state once the deck opens and once edits stop, laid out away
/// from the caller's thread so the window answers meanwhile (PLAN 4.13). Each edit that changes
/// the deck gives back the source it replaced, which `restore` makes the deck again: an undo, or
/// its redo.
///
/// Use it from one thread at a time, as the session: the window's, the main actor.
@Observable
public final class DeckEditor {
    public let session: ScaenaSession

    /// Bumped by each change to the deck: what draws it again.
    public private(set) var revision = 0
    /// Bumped when what frames at rest draw changes without an edit: a node drawn moved by a
    /// drag, or a patch shown before it is made (PLAN 3.7). What draws the canvas again.
    public private(set) var drawn = 0
    /// Changed by each edit and by each change to what frames at rest draw without one: the
    /// revision the canvas paints by (`ScaenaCanvas`), and is told it painted.
    public var redraws: Int { revision &+ drawn }
    /// The deck's states on its timeline, each step named by its slide.
    public private(set) var slots: [ScaenaSession.Slot] = []
    /// The source as the source pane shows it: the deck's, or what is typed.
    public private(set) var source = ""
    /// Why the source does not compile, where it does not.
    public private(set) var error: Finding?
    /// Whether the source compiles to a deck that validates: the deck shown, which a gesture edits.
    public private(set) var valid = false
    /// What compiling and lint found, in every format the deck lists.
    public private(set) var findings: [Finding] = []
    /// Whether lint ran on every state, or on the state shown with the rest kept.
    public private(set) var whole = false
    /// Whether every state is being linted away from the window (PLAN 4.13): its findings come
    /// when it is done, where the deck has not changed since it began.
    public var linting: Bool { lints > 0 }
    /// The lints of every state begun and not yet done.
    private var lints = 0
    /// The state the window shows: where an edit is linted at once.
    public var shown: String?
    /// The format frames are laid out in (PLAN 2.62, 3.16): one of the deck's `formats`, or none
    /// for its own canvas.
    public private(set) var format: String?

    /// Why the deck shows no state, where it shows none: it opened, but its source does not
    /// compile or validate, so it is no deck to show or edit yet. What the window says in place
    /// of a slide, the source pane open to mend it.
    public var unshown: String? {
        guard slots.isEmpty, !valid else { return nil }
        let first = error ?? findings.first { $0.severity == .error }
        return first.map { [$0.message, $0.hint].compactMap(\.self).joined(separator: ". ") }
            ?? "Its source does not compile."
    }

    /// The state `by` places from `state` along the deck's timeline, a slide or a step (PLAN 4.12,
    /// as the browser's PageDown and PageUp go, PLAN 2.126): the next, `1`, or the one before, `-1`.
    /// None past either end, or where the deck has no such state.
    public func state(from state: String, by: Int) -> String? {
        guard let at = slots.firstIndex(where: { $0.state == state }), slots.indices.contains(at + by) else {
            return nil
        }
        return slots[at + by].state
    }

    /// The source the deck was made from last: what an undo of the next edit makes it again.
    @ObservationIgnored private var made = ""
    /// Bumped by each source compiled: a lint of every state begun before it is of another deck.
    @ObservationIgnored private var compiled = 0
    /// Where every state is linted, away from the window (PLAN 4.13).
    @ObservationIgnored private let linter = Linter()
    /// Each state drawn small, by the digest of its drawing at rest and the width asked.
    @ObservationIgnored private var drawings: [String: (digest: String, width: Int, image: CGImage)] = [:]
    /// Each slide to start as its layout's picture shows it (PLAN 3.31), by its layout, `""` the
    /// blank slide: for the theme, the format, and the height they were drawn in, the `key`.
    @ObservationIgnored private var started: (key: String, drawn: [String: (starter: Starter, image: CGImage)]) = ("", [:])

    public init(session: ScaenaSession) {
        self.session = session
        // Compiled as the browser opens a deck. Its every state is linted once the window shows
        // it (`lintEvery`), away from the window: opening waits on no lint (PLAN 4.13).
        let source = (try? session.source()) ?? ""
        _ = take(source)
    }

    /// Make `ops`, a patch, as `author` (ADR-0013): the source the deck had, for an undo. A patch
    /// that changes nothing, or that the deck refuses, throws why; so does one made while the
    /// source does not compile, which is no deck to edit.
    @discardableResult
    public func make(_ ops: [JSONValue], author: String = "user") throws -> String {
        guard valid, error == nil else { throw ScaenaError(message: "the source does not compile: fix it first") }
        let called = try session.tool("deck_patch", ["ops": .array(ops)], author: author)
        guard called.edited else {
            // What the deck refused, as the browser says it (PLAN 2.99, 3.18): what lint found, in
            // the sentence an editor says it in, never lint's own message, which names what it
            // finds by the ids the deck writes.
            guard let refused = called.result["added"]?.array?.first(where: { $0["severity"]?.string == "error" }) else {
                throw ScaenaError(message: "it is there already")
            }
            throw ScaenaError(message: "the deck would not open with it. \(refusal(refused))")
        }
        let before = made
        let next = try session.source()
        _ = take(next)
        return before
    }

    /// What lint found in a change refused, as an editor says it (`scaena_core::words`, as the
    /// browser's `finding` says it): the object it is about, by what it shows in its state, else in
    /// the state shown, or the deck or its theme; then what it finds, in a sentence. A patch, a
    /// theme taken or edited, a version restored, and a data edit are each refused so (PLAN 3.18).
    public func refusal(_ finding: JSONValue) -> String {
        let state = finding["state"]?.string ?? shown ?? slots.first?.state ?? ""
        let words = try? session.call("words", ["state": .string(state)], as: JSONValue.self)
        let code = finding["code"]?.string ?? ""
        let found =
            finding["said"]?.string ?? words?["issues"]?[code]?.string
            ?? "Something in it isn't written the way the deck allows."
        let about: String
        if let node = finding["node"]?.string, let shows = words?["objects"]?[node]?.string {
            about = shows == words?["kinds"]?[node]?.string ? "The \(shows.lowercased())" : "“\(shows)”"
        } else {
            about = finding["file"]?.string == nil ? "The deck" : "The theme"
        }
        return "\(about): \(found)"
    }

    /// A new deck's one blank state made a title slide in the theme's `title` layout, its words to
    /// type over and its picture's place waiting, as a presentation app's new deck opens (PLAN
    /// 3.30): the slide's id. None where the deck is not one state with nothing on it, or the theme
    /// has no such layout. Made as the deck is made, before anything is undone to.
    @discardableResult
    public func startOnTitleSlide() -> String? {
        guard slots.count == 1, let blank = slots.first?.state,
            (try? session.boxes(state: blank))?.isEmpty == true,
            let started = try? session.starting(after: blank, layout: "title"),
            (try? make(started.patch + [["op": "remove_state", "id": .string(blank)]])) != nil
        else { return nil }
        return started.id
    }

    /// Make `ops`, text typed in place on the canvas (PLAN 3.9), as the user at `date`:
    /// `replace_text`, `style_text`, or `list` ops, validated as a patch is but not linted, as the
    /// browser's typing is: the findings shown stay until edits stop and `lintEvery` runs. The
    /// source the deck had, for an undo. What reads so already, or what the deck refuses, throws
    /// why.
    @discardableResult
    public func typed(_ ops: [JSONValue], at date: Date = Date()) throws -> String {
        guard valid, error == nil else { throw ScaenaError(message: "the source does not compile: fix it first") }
        guard try session.typed(ops, at: date) else { throw ScaenaError(message: "it reads so already") }
        guard let before = take(try session.source(), lint: false) else {
            throw ScaenaError(message: error?.said ?? "the deck typed does not validate")
        }
        return before
    }

    /// Compile `typed`, the source pane's text: the source the deck had, for an undo, where it
    /// became the deck; none where it does not compile or validate, or says what the pane did.
    public func type(_ typed: String) -> String? {
        guard typed != source else { return nil }
        return take(typed)
    }

    /// Apply `finding`'s fix (SPEC §7.4): the source the deck had, for an undo.
    @discardableResult
    public func fix(_ finding: Finding) throws -> String {
        guard let patch = finding.fix, finding.fixable else { throw ScaenaError(message: "there is no fix for it") }
        let fixed = try session.fix(patch)
        guard let before = take(fixed) else {
            throw ScaenaError(message: error?.said ?? "the fixed deck does not validate")
        }
        return before
    }

    /// Make the deck `source` again (an undo, or its redo): the source it replaces.
    @discardableResult
    public func restore(_ source: String) -> String? {
        take(source)
    }

    /// Draw `nodes` moved `by` canvas units in frames at rest, laying nothing out: a drag as it
    /// moves (ADR-0013).
    public func move(_ nodes: [String], by: CGVector) {
        try? session.setMoving(nodes, by: by)
        drawn += 1
    }

    /// Draw frames at rest as `ops` would make the deck, nothing made: a resize that paused.
    public func show(_ ops: [JSONValue]) {
        try? session.preview(ops)
        drawn += 1
    }

    /// Lay frames out in `format` from now on, one of the deck's formats, or on its own canvas
    /// where none (PLAN 2.62, 3.16): the canvas, its boxes and targets, and what lint says holds
    /// there follow it. A gesture there moves a node with a layout of its own there alone (PLAN
    /// 2.85). One the deck does not list throws why, the format shown as it was.
    public func show(format: String?) throws {
        try session.setFormat(format)
        self.format = format
        revision += 1
    }

    /// Paint the canvas through `view`, the part of it shown, canvas units: zoomed in (PLAN 2.46,
    /// 3.16). None paints the whole canvas.
    public func look(through view: CGRect?) {
        try? session.setView(view)
        drawn += 1
    }

    /// A drag over: every node drawn where it stands, and nothing shown before it is made.
    public func still() {
        try? session.setMoving([])
        try? session.preview(nil)
        drawn += 1
    }

    /// Take the deck as the session holds it after an edit made on it directly, as the assistant
    /// makes one (PLAN 3.6), or a panel beside the source: the theme edited or another taken, a
    /// data file written, a version restored (PLAN 3.15). It is compiled and linted as any edit
    /// is, and drawn again: the source it replaces, for an undo.
    @discardableResult
    public func reread() -> String? {
        guard let next = try? session.source() else { return nil }
        return take(next)
    }

    /// Lint the state shown, laid out alone, the others' findings kept: what an edit shows at once.
    /// With no state shown, what compiling found stands until `lintEvery` comes back.
    public func lintShown() {
        guard let shown, let linting = try? session.lint(state: shown) else { return }
        findings = linting.findings
        whole = linting.whole
    }

    /// Lint every state, in every format: once the deck opens, and once edits stop (PLAN 4.13).
    /// The deck as it stands is laid out on a queue of its own, one lint at a time, while the
    /// caller's actor (the window's) goes on answering; its findings are taken on that actor
    /// when it is done. A lint of a deck an edit has changed since it began is dropped, so what
    /// an older deck had never replaces what lint found in this one.
    public func lintEvery(isolation: isolated (any Actor)? = #isolation) async {
        guard valid, let laying = session.laying() else { return }
        let began = compiled
        lints += 1
        defer { lints -= 1 }
        await linter.lay(laying, for: began)
        guard began == compiled, let linting = try? session.lint(laid: laying) else { return }
        findings = linting.findings
        whole = linting.whole
    }

    /// `state` drawn at rest `width` pixels wide by the CPU painter: drawn again only when its
    /// drawing changed, as the browser's strip draws it (PLAN 2.35).
    public func drawing(_ state: String, width: Int) -> CGImage? {
        guard let digest = try? session.digest(state: state) else { return nil }
        if let kept = drawings[state], kept.digest == digest, kept.width == width { return kept.image }
        guard let image = try? session.pixels(state, width: width).image else { return nil }
        drawings[state] = (digest, width, image)
        return image
    }

    /// The slides `layouts` start after the slide of `state`, a nil among them the blank one, each
    /// drawn as its layout's picture shows it, `height` pixels high (PLAN 3.31): those drawn before
    /// in this theme and format kept, the rest drawn now. A slide started draws the same after any
    /// slide, so what is kept stands until the theme, the format, or the height changes. In the
    /// order asked; a name the theme lacks, or a picture that cannot be made, is left out. A picker
    /// asks for a few at a time, answering a person between them.
    public func starters(_ layouts: [String?], after state: String, height: Int) -> [(starter: Starter, image: CGImage)] {
        let key = startersKey(height: height)
        if started.key != key { started = (key, [:]) }
        var asked = Set<String>()
        let missing = layouts.filter { started.drawn[$0 ?? ""] == nil && asked.insert($0 ?? "").inserted }
        if !missing.isEmpty {
            for one in (try? session.startersDrawn(after: state, height: height, only: missing)) ?? [] {
                started.drawn[one.starter.layout ?? ""] = one
            }
        }
        return layouts.compactMap { started.drawn[$0 ?? ""] }
    }

    /// The slides to start drawn so far for `layouts`, a nil among them the blank one, as
    /// `starters` drew them in this theme and format: what a picker shows at once.
    public func startersKept(_ layouts: [String?], height: Int) -> [(starter: Starter, image: CGImage)] {
        guard started.key == startersKey(height: height) else { return [] }
        return layouts.compactMap { started.drawn[$0 ?? ""] }
    }

    /// What slides to start are drawn in: the theme, the format, and the height.
    private func startersKey(height: Int) -> String {
        let theme = (try? session.themeText())?.text ?? ""
        return "\(theme.hashValue)\u{1f}\(format ?? "")\u{1f}\(height)"
    }

    /// Compile `next` as the source: the source the deck had, where `next` became the deck. With
    /// `lint`, the state shown is linted at once; without, the findings shown wait for `lintEvery`.
    private func take(_ next: String, lint: Bool = true) -> String? {
        source = next
        compiled += 1
        linter.latest.store(compiled, ordering: .relaxed)
        guard let compiling = try? session.compile(next) else {
            error = nil
            valid = false
            return nil
        }
        error = compiling.error
        valid = compiling.valid && compiling.error == nil
        guard valid else {
            // The deck shown stays; what is wrong with the source is what there is to show.
            findings = compiling.error.map { [$0] } ?? compiling.findings
            whole = false
            return nil
        }
        let before = made
        made = next
        slots = (try? session.timeline()) ?? slots
        if let shown, !slots.contains(where: { $0.state == shown }) { self.shown = slots.first?.state }
        // A format the deck no longer lists: the session lays frames out on its own canvas again.
        if let format, !((try? session.formats()) ?? []).contains(format) { self.format = nil }
        revision += 1
        guard lint else {
            whole = false
            return before
        }
        findings = compiling.findings
        lintShown()
        return before
    }
}

/// Where a deck's every state is linted, away from the window (PLAN 4.13): one lint at a time on
/// a queue of its own, so lints begun as edits stop wait for the one before rather than share the
/// machine with it, and one of a deck an edit has changed since it was begun is not laid out.
private final class Linter: Sendable {
    private let queue = DispatchQueue(label: "ScaenaKit.lint", qos: .utility)
    /// The deck compiled last, as `DeckEditor` counts them.
    let latest = Atomic<Int>(0)

    /// Lay `laying` out, taken from the deck `began` counts, on the queue: once the lints before
    /// it are done, and only where no other deck has been compiled since.
    func lay(_ laying: Laying, for began: Int) async {
        await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
            queue.async {
                if self.latest.load(ordering: .relaxed) == began { laying.lay() }
                done.resume()
            }
        }
    }
}
