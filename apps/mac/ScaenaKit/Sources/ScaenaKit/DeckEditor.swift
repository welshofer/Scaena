import CoreGraphics
import Foundation
import Observation

/// A deck open for editing, as the browser's editor keeps one (PLAN 2.3, 3.4): the session, the
/// deck as `.scn` as the source pane shows it, and what compiling and lint found.
///
/// The source is what the deck is made from. A patch (a gesture, a choice in the inspector) makes
/// the deck's source again, a finding's fix rewrites it, and a source typed is compiled as it
/// stands: each is compiled, becomes the deck where it validates, and lints the state shown at
/// once; `lintEvery` lints every state once edits stop. Each edit that changes the deck gives
/// back the source it replaced, which `restore` makes the deck again: an undo, or its redo.
///
/// Use it from one thread at a time, as the session.
@Observable
public final class DeckEditor {
    public let session: ScaenaSession

    /// Bumped by each change to the deck: what draws it again.
    public private(set) var revision = 0
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
    /// The state the window shows: where an edit is linted at once.
    public var shown: String?

    /// The source the deck was made from last: what an undo of the next edit makes it again.
    @ObservationIgnored private var made = ""
    /// Each state drawn small, by the digest of its drawing at rest and the width asked.
    @ObservationIgnored private var drawings: [String: (digest: String, width: Int, image: CGImage)] = [:]

    public init(session: ScaenaSession) {
        self.session = session
        // Compiled as the browser opens a deck, and linted on every state, none being shown yet.
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
            // What the deck refused, as the browser says it.
            let refused = called.result["added"]?.array?.first { $0["severity"]?.string == "error" }
            let why = refused?["message"]?.string.map { "the deck refuses it: \($0)" } ?? "it is there already"
            throw ScaenaError(message: why)
        }
        let before = made
        let next = try session.source()
        _ = take(next)
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
        guard let patch = finding.fix, finding.fixable else { throw ScaenaError(message: "\(finding.code) has no fix") }
        let fixed = try session.fix(patch)
        guard let before = take(fixed) else {
            throw ScaenaError(message: error?.message ?? "the fixed deck does not validate")
        }
        return before
    }

    /// Make the deck `source` again (an undo, or its redo): the source it replaces.
    @discardableResult
    public func restore(_ source: String) -> String? {
        take(source)
    }

    /// Lint the state shown, laid out alone, the others' findings kept: what an edit shows at once.
    public func lintShown() {
        guard let shown else { return lintEvery() }
        guard let linting = try? session.lint(state: shown) else { return }
        findings = linting.findings
        whole = linting.whole
    }

    /// Lint every state, in every format: once edits stop.
    public func lintEvery() {
        guard let linting = try? session.lint() else { return }
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

    /// Compile `next` as the source: the source the deck had, where `next` became the deck.
    private func take(_ next: String) -> String? {
        source = next
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
        revision += 1
        findings = compiling.findings
        lintShown()
        return before
    }
}
