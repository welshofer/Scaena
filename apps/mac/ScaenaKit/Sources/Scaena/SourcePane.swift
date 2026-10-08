import AppKit
import SwiftUI

/// The deck's `.scn` (PLAN 3.4, SPEC §4), as the browser's source pane shows it: what is typed is
/// compiled once typing pauses, and becomes the deck where it validates; an edit made elsewhere (a
/// gesture, a choice, a fix, an undo) writes the deck's source back here. The pane keeps no undo
/// of its own: each source that becomes the deck is one step of the document's.
///
/// Source is chrome: the canvas's text is the engine's, never laid out here (SPEC §9.3).
struct SourcePane: NSViewRepresentable {
    /// The source as the editor has it.
    let text: String
    /// Told the pane's text once typing pauses.
    let typed: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        guard let view = scroll.documentView as? NSTextView else { return scroll }
        view.isRichText = false
        view.allowsUndo = false
        view.usesFindBar = true
        view.isIncrementalSearchingEnabled = true
        view.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        view.textContainerInset = NSSize(width: 6, height: 8)
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.isContinuousSpellCheckingEnabled = false
        view.isGrammarCheckingEnabled = false
        view.string = text
        view.delegate = context.coordinator
        context.coordinator.typed = typed
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.typed = typed
        guard let view = scroll.documentView as? NSTextView else { return }
        // The deck changed elsewhere: its source is written back, the selection kept where it can be.
        guard !context.coordinator.typing, view.string != text else { return }
        let selected = view.selectedRange()
        view.string = text
        let length = (text as NSString).length
        let location = min(selected.location, length)
        view.setSelectedRange(NSRange(location: location, length: min(selected.length, length - location)))
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var typed: (String) -> Void = { _ in }
        /// Whether typing waits to be compiled: the pane's text is ahead of the editor's.
        var typing = false
        private var pending: DispatchWorkItem?

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            typing = true
            pending?.cancel()
            let work = DispatchWorkItem { [weak self, weak view] in
                guard let self, let view else { return }
                self.typing = false
                self.typed(view.string)
            }
            pending = work
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(250), execute: work)
        }
    }
}
