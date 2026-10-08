import ScaenaKit
import SwiftUI

/// A deck's window: its states down the side, as the timeline plays them, and the one selected
/// painted by the engine on Metal, its cue played as it is chosen (PLAN 3.2–3.3).
struct DeckView: View {
    @ObservedObject var document: ScaenaDocument
    @Environment(\.undoManager) private var undo
    @State private var selection: String?
    @State private var failure: String?

    var body: some View {
        let slots = (try? document.session.timeline()) ?? []
        let shown = selection.flatMap { s in slots.first { $0.state == s }?.state } ?? slots.first?.state
        NavigationSplitView {
            List(slots, id: \.state, selection: $selection) { slot in
                VStack(alignment: .leading, spacing: 2) {
                    Text(slot.state)
                    if slot.slide != slot.state {
                        Text("step of \(slot.slide)").font(.caption).foregroundStyle(.secondary)
                    }
                }
                .contextMenu {
                    Button("Duplicate Slide") { make(["op": "duplicate_slide", "slide": .string(slot.slide)]) }
                    Button("Delete Slide", role: .destructive) {
                        make(["op": "remove_slide", "slide": .string(slot.slide)])
                    }
                }
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
        } detail: {
            if let shown {
                ScaenaCanvas(session: document.session, state: shown, revision: document.revision)
                    .aspectRatio(aspect, contentMode: .fit)
                    .padding()
            } else {
                ContentUnavailableView("No states", systemImage: "rectangle.stack")
            }
        }
        .alert("Not made", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } }), presenting: failure) { _ in
            Button("OK") { failure = nil }
        } message: { Text($0) }
    }

    /// The canvas's aspect: the deck's, or its format's.
    private var aspect: CGFloat {
        let size = (try? document.session.canvasSize()) ?? CGSize(width: 16, height: 9)
        return size.height > 0 ? size.width / size.height : 16 / 9
    }

    private func make(_ op: JSONValue) {
        do {
            try document.make([op], undo: undo)
        } catch {
            failure = "\(error)"
        }
    }
}
