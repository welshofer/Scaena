import ScaenaKit
import SwiftUI

/// A rehearsal in the window (PLAN 2.63, 3.14), in place of the canvas: the deck played as
/// presented, from its first state, each state's cue as it comes, then the state at rest until the
/// presenter goes on. → ↓ Page Down Space Return or a click go on; ← ↑ Page Up Delete go back;
/// Escape stops, as does going on from the last state. The bar says which state is shown, how
/// long it has been shown, and how long the rehearsal has run. Nothing changes the deck until
/// Keep, in what each state took (`RehearsedSheet`).
struct RehearsalStage: View {
    let editor: DeckEditor
    @Binding var rehearsal: Rehearsal
    /// The rehearsal has stopped.
    let stopped: () -> Void
    @State private var playhead = Playhead()
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            bar
            Divider()
            if let state = rehearsal.state {
                let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
                ScaenaCanvas(session: editor.session, state: state, revision: editor.revision, playhead: $playhead)
                    .aspectRatio(size.width / max(size.height, 1), contentMode: .fit)
                    .overlay { Color.clear.contentShape(Rectangle()).onTapGesture { go(1) } }
                    .padding()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(keys: [.rightArrow, .downArrow, .pageDown, .space, .return]) { _ in
            go(1)
            return .handled
        }
        .onKeyPress(keys: [.leftArrow, .upArrow, .pageUp, .delete]) { _ in
            go(-1)
            return .handled
        }
        .onKeyPress(.escape) {
            stop()
            return .handled
        }
        .onAppear { focused = true }
    }

    /// Where the rehearsal is, said four times a second, and its buttons.
    private var bar: some View {
        HStack(spacing: 12) {
            TimelineView(.periodic(from: .now, by: 0.25)) { context in
                Text(whereItIs(at: context.date)).monospacedDigit()
            }
            Spacer()
            Button("Back") { go(-1) }
            Button("On") { go(1) }
            Button("End") { stop() }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }

    private func whereItIs(at _: Date) -> String {
        let n = rehearsal.slots.count
        let here = rehearsal.slots.indices.contains(rehearsal.index) ? rehearsal.index + 1 : n
        return "Rehearsing \(rehearsal.state ?? "") (\(here) of \(n)) · \(readable(rehearsal.here())) here · "
            + "\(readable(rehearsal.total())) in all"
    }

    private func go(_ by: Int) {
        guard rehearsal.running else { return }
        rehearsal.go(by)
        guard rehearsal.running else { return stopped() }
        playhead = Playhead()
    }

    private func stop() {
        guard rehearsal.running else { return }
        rehearsal.stop()
        stopped()
    }
}

/// What each state took in a rehearsal (PLAN 2.63): how long it was shown, its cue, the hold that
/// time keeps, and its hold now. Keep makes each state reached hold that long, as one patch.
struct RehearsedSheet: View {
    let rehearsal: Rehearsal
    let keep: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("What each state took").font(.headline)
            Text(
                "\(readable(rehearsal.total())) in all. A state's cue plays, then its hold: the hold kept is the "
                    + "time less its cue.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            ScrollView {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 4) {
                    GridRow {
                        Text("State")
                        Text("Shown")
                        Text("Cue")
                        Text("Keeps")
                        Text("Hold now")
                    }
                    .font(.caption.bold())
                    ForEach(Array(rehearsal.kept.enumerated()), id: \.offset) { _, row in
                        GridRow {
                            Text(row.state)
                            Text(row.spent > 0 ? readable(row.spent) : "not reached")
                            Text(readable(row.span))
                            Text(row.keeps.map(readable) ?? "as it is")
                            Text(row.hold > 0 ? readable(row.hold) : "none")
                        }
                        .monospacedDigit()
                    }
                }
            }
            HStack {
                Spacer()
                Button("Discard", role: .cancel) { dismiss() }
                Button("Keep") {
                    keep()
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(rehearsal.holds.isEmpty)
            }
        }
        .padding()
        .frame(minWidth: 480, minHeight: 360)
    }
}

/// `ms` as a rehearsal says it: `1:05`, or `4.2 s` under a minute.
func readable(_ ms: Double) -> String {
    let s = ms / 1000
    if s < 60 { return String(format: "%.1f s", s) }
    let m = Int(s / 60)
    return String(format: "%d:%02d", m, Int(s) - m * 60)
}
