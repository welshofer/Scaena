import ScaenaKit
import SwiftUI

/// The slides down the side (PLAN 3.4), as a presentation app's navigator (PLAN 3.18): each slide
/// numbered and drawn small at rest by the engine, painted again only when its drawing changes, as
/// the browser's strip is (PLAN 2.35, 3.14); a slide's steps under it, set in. A slide or a step is
/// added after one, moved by a drag or a step up or down, and removed; a slide copied or taken out.
/// Each is one patch, one step to undo. On the Mac, Delete takes out the slide chosen, with its
/// steps, or the step chosen, as a presentation app's navigator does, and the one after it shows;
/// Return adds a slide after it.
struct StateList: View {
    let editor: DeckEditor
    @Binding var chosen: String?
    /// Make `ops`, one step to undo, then show `then`, where it is a state.
    let make: ([JSONValue], String?) -> Void
    /// Add a step or a slide after a state, then show it.
    let add: (String, StateAdding) -> Void

    /// Each state with its slide's number, and its place among the slide's steps from 1.
    private var places: [String: (slide: Int, step: Int)] {
        var places: [String: (slide: Int, step: Int)] = [:]
        var number = 0
        var step = 0
        var slide: String?
        for slot in editor.slots {
            if slot.slide != slide {
                number += 1
                step = 1
                slide = slot.slide
            } else {
                step += 1
            }
            places[slot.state] = (number, step)
        }
        return places
    }

    var body: some View {
        let numbered = places
        let list = List(selection: $chosen) {
            ForEach(editor.slots, id: \.state) { slot in
                let place = numbered[slot.state] ?? (slide: 0, step: 1)
                StateRow(editor: editor, slot: slot, number: place.slide, step: place.step, revision: editor.revision)
                    .contextMenu { menu(slot) }
            }
            .onMove(perform: move)
        }
        #if os(macOS)
        list.onDeleteCommand(perform: deleteChosen)
            .onKeyPress(.return, action: newSlide)
        #else
        list.onKeyPress(.return, action: newSlide)
        #endif
    }

    /// Return adds a slide after the slide chosen and shows it, as a presentation app's navigator
    /// does.
    private func newSlide() -> KeyPress.Result {
        guard let chosen else { return .ignored }
        add(chosen, .slide)
        return .handled
    }

    /// The row chosen taken out: its slide, with its steps, where it is a slide's first state;
    /// else the step. The deck's only slide stays.
    private func deleteChosen() {
        guard let chosen, let slot = editor.slots.first(where: { $0.state == chosen }) else { return }
        if places[chosen]?.step == 1 {
            guard Slide.of(editor.slots).count > 1 else { return }
            make(Restaging.removeSlides([slot.slide]), shownAfter(removing: slot.slide))
        } else {
            remove(chosen)
        }
    }

    /// What shows once `slide` is taken out: the slide after it, or, past the last, the one before.
    private func shownAfter(removing slide: String) -> String? {
        let slides = Slide.of(editor.slots)
        guard let i = slides.firstIndex(where: { $0.id == slide }) else { return nil }
        let next = i + 1 < slides.count ? i + 1 : i - 1
        return slides.indices.contains(next) ? slides[next].states.first : nil
    }

    /// What is done to a slide, or a step of it.
    @ViewBuilder private func menu(_ slot: ScaenaSession.Slot) -> some View {
        let states = editor.slots.map(\.state)
        let steps = editor.slots.filter { $0.slide == slot.slide }.count
        Button("New Slide", systemImage: "plus.rectangle.on.rectangle") { add(slot.state, .slide) }
        Button("New Step on This Slide", systemImage: "plus.square.dashed") { add(slot.state, .step) }
        Divider()
        Button("Duplicate Slide", systemImage: "plus.square.on.square") {
            make(Restaging.duplicateSlides([slot.slide]), nil)
        }
        Button("Move Up", systemImage: "arrow.up") { step(slot.state, by: -1) }
            .disabled(states.first == slot.state)
        Button("Move Down", systemImage: "arrow.down") { step(slot.state, by: 1) }
            .disabled(states.last == slot.state)
        Divider()
        if steps > 1 {
            Button("Delete Step", systemImage: "minus.square", role: .destructive) { remove(slot.state) }
        }
        Button("Delete Slide", systemImage: "trash", role: .destructive) {
            make(Restaging.removeSlides([slot.slide]), shownAfter(removing: slot.slide))
        }
        .disabled(Slide.of(editor.slots).count < 2)
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
}

/// A slide drawn small with its number beside it, as a navigator shows it; a step of it drawn
/// smaller, set in under it, with no number.
private struct StateRow: View {
    let editor: DeckEditor
    let slot: ScaenaSession.Slot
    /// Its slide's number, from 1.
    let number: Int
    /// Its place among its slide's steps, from 1: the slide itself is 1.
    let step: Int
    let revision: Int
    @State private var drawing: CGImage?
    @Environment(\.displayScale) private var scale

    /// The width a drawing is shown at, points: a step's narrower.
    private var width: CGFloat { step > 1 ? 132 : 168 }

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Text(step > 1 ? "" : "\(number)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: 20, alignment: .trailing)
                .padding(.top, 2)
            Group {
                if let drawing {
                    Image(decorative: drawing, scale: scale).resizable().aspectRatio(contentMode: .fit)
                } else {
                    Rectangle().fill(.quaternary).aspectRatio(16 / 9, contentMode: .fit)
                }
            }
            .frame(maxWidth: width)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(.separator))
            .padding(.leading, step > 1 ? 24 : 0)
        }
        .padding(.vertical, step > 1 ? 1 : 4)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(step > 1 ? "Slide \(number), step \(step)" : "Slide \(number)")
        .accessibilityIdentifier("state-\(slot.state)")
        .task(id: "\(slot.state)\u{1f}\(revision)") {
            drawing = editor.drawing(slot.state, width: Int(width * scale))
        }
    }
}
