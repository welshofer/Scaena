import ScaenaKit
import SwiftUI

/// The deck's states as the timeline plays them (PLAN 3.4), each drawn small at rest by the
/// engine and painted again only when its drawing changes, as the browser's strip is (PLAN 2.35,
/// 3.14): a step or a slide added after a state, a state renamed, moved by a drag or a step up or
/// down, and removed; a slide copied or taken out. Each is one patch, one step to undo.
struct StateList: View {
    let editor: DeckEditor
    @Binding var chosen: String?
    /// Make `ops`, one step to undo, then show `then`, where it is a state.
    let make: ([JSONValue], String?) -> Void
    /// Add a step or a slide after a state, then show it.
    let add: (String, StateAdding) -> Void
    /// The state asked to be renamed, and its name as the field has it.
    @State private var renaming: String?
    @State private var name = ""

    var body: some View {
        List(selection: $chosen) {
            ForEach(editor.slots, id: \.state) { slot in
                StateRow(editor: editor, slot: slot, revision: editor.revision)
                    .contextMenu { menu(slot) }
            }
            .onMove(perform: move)
        }
        .safeAreaInset(edge: .bottom) { adding }
        .alert("Rename \(renaming ?? "")", isPresented: asking) {
            TextField("Name", text: $name)
            Button("Rename") { rename() }
            Button("Cancel", role: .cancel) { renaming = nil }
        } message: {
            Text("A lowercase letter, then lowercase letters, digits, - and _. Its links are renamed with it.")
        }
    }

    /// What is done to a state, and to its slide.
    @ViewBuilder private func menu(_ slot: ScaenaSession.Slot) -> some View {
        let states = editor.slots.map(\.state)
        Button("Add Step After") { add(slot.state, .step) }
        Button("Add Slide After") { add(slot.state, .slide) }
        Divider()
        Button("Rename…") {
            name = slot.state
            renaming = slot.state
        }
        Button("Move Up") { step(slot.state, by: -1) }
            .disabled(states.first == slot.state)
        Button("Move Down") { step(slot.state, by: 1) }
            .disabled(states.last == slot.state)
        Button("Remove State", role: .destructive) { remove(slot.state) }
            .disabled(states.count < 2)
        Divider()
        Button("Duplicate Slide") { make(Restaging.duplicateSlides([slot.slide]), nil) }
        Button("Delete Slide", role: .destructive) { make(Restaging.removeSlides([slot.slide]), nil) }
            .disabled(Slide.of(editor.slots).count < 2)
    }

    /// Add a step or a slide after the state shown.
    private var adding: some View {
        let shown = chosen ?? editor.slots.first?.state
        return HStack {
            Menu {
                Button("Add Step") { if let shown { add(shown, .step) } }
                Button("Add Slide") { if let shown { add(shown, .slide) } }
            } label: {
                Label("Add", systemImage: "plus")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .disabled(shown == nil)
            .help("Add a step of the state shown's slide after it, or a slide of its own after its slide")
            Spacer()
        }
        .padding(8)
    }

    private var asking: Binding<Bool> {
        Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
    }

    /// A state dragged to `to`, a place in the list as it was: just before the state there, or
    /// just after the last.
    private func move(from: IndexSet, to: Int) {
        let states = editor.slots.map(\.state)
        guard from.count == 1, let i = from.first, states.indices.contains(i), to != i, to != i + 1 else { return }
        let state = states[i]
        if to < states.count {
            make(Restaging.move(state, before: true, states[to]), state)
        } else if let last = states.last {
            make(Restaging.move(state, before: false, last), state)
        }
    }

    /// `state` a place up (`by` -1) or down.
    private func step(_ state: String, by: Int) {
        let states = editor.slots.map(\.state)
        guard let i = states.firstIndex(of: state), states.indices.contains(i + by) else { return }
        make(Restaging.move(state, before: by < 0, states[i + by]), state)
    }

    /// `state` removed: the state after it shows, or, past the last, the one before.
    private func remove(_ state: String) {
        let states = editor.slots.map(\.state)
        guard let i = states.firstIndex(of: state), states.count > 1 else { return }
        make(Restaging.remove(state), i + 1 < states.count ? states[i + 1] : states[i - 1])
    }

    private func rename() {
        guard let from = renaming else { return }
        renaming = nil
        let to = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !to.isEmpty, to != from else { return }
        make(Restaging.rename(from, to: to), to)
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
        .task(id: "\(slot.state)\u{1f}\(revision)") {
            drawing = editor.drawing(slot.state, width: Int(width * scale))
        }
    }
}
