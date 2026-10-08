import ScaenaKit
import SwiftUI

/// The deck's states as the timeline plays them (PLAN 3.4), each drawn small at rest by the
/// engine and painted again only when its drawing changes, as the browser's strip is (PLAN
/// 2.35). A slide's context menu duplicates or deletes it, each a patch.
struct StateList: View {
    let editor: DeckEditor
    @Binding var chosen: String?
    let make: (JSONValue) -> Void

    var body: some View {
        List(editor.slots, id: \.state, selection: $chosen) { slot in
            StateRow(editor: editor, slot: slot, revision: editor.revision)
                .contextMenu {
                    Button("Duplicate Slide") { make(["op": "duplicate_slide", "slide": .string(slot.slide)]) }
                    Button("Delete Slide", role: .destructive) {
                        make(["op": "remove_slide", "slide": .string(slot.slide)])
                    }
                }
        }
    }
}

/// A state drawn small, its name, and, for a step, its slide's.
private struct StateRow: View {
    let editor: DeckEditor
    let slot: ScaenaSession.Slot
    let revision: Int
    @State private var drawing: CGImage?
    @Environment(\.displayScale) private var scale

    /// The width a drawing is shown at, points.
    private let width: CGFloat = 176

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Group {
                if let drawing {
                    Image(decorative: drawing, scale: scale).resizable().aspectRatio(contentMode: .fit)
                } else {
                    Rectangle().fill(.quaternary).aspectRatio(16 / 9, contentMode: .fit)
                }
            }
            .frame(maxWidth: width)
            .clipShape(RoundedRectangle(cornerRadius: 3))
            .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(.separator))
            HStack(spacing: 4) {
                Text(slot.state)
                if slot.slide != slot.state {
                    Text("step of \(slot.slide)").foregroundStyle(.secondary)
                }
            }
            .font(.caption)
            .lineLimit(1)
        }
        .padding(.vertical, 3)
        .task(id: revision) {
            drawing = editor.drawing(slot.state, width: Int(width * scale))
        }
    }
}
