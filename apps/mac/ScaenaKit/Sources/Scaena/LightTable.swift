import ScaenaKit
import SwiftUI

/// The light table (PLAN 2.97, 3.14), in place of the canvas: every slide of the deck, each drawn
/// at its last state at rest, as the PDF draws it, to reorder, copy, and take out across a large
/// deck. A click selects a slide, Shift+click every slide from the one last clicked, ⌘-click adds
/// one or takes it away; ← and → select the slide before or after, Home and End the first and the
/// last. The slides selected, dragged onto another, go before it where they come after it and after
/// it where they come before; ⌥← and ⌥→ move them a place; ⌘D (the Node menu's Duplicate) copies
/// them, each just after itself; Delete takes them out. Return, or a double click, shows a slide's
/// first state on the canvas. Each is one patch, and every other state shows what it showed.
struct LightTable: View {
    let editor: DeckEditor
    /// The slides selected.
    @Binding var picked: Set<String>
    /// Show `state` on the canvas, the table put away.
    let show: (String) -> Void
    /// Make `ops`, one step to undo, then select the slides `then`.
    let make: ([JSONValue], [String]) -> Void
    /// The slide last clicked: where Shift+click selects from.
    @State private var anchor: String?
    @FocusState private var focused: Bool

    var body: some View {
        let slides = Slide.of(editor.slots)
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 200, maximum: 320), spacing: 16)], spacing: 16) {
                ForEach(Array(slides.enumerated()), id: \.element.id) { n, slide in
                    SlideCell(
                        editor: editor, slide: slide, number: n + 1, picked: picked.contains(slide.id),
                        revision: editor.revision
                    )
                    .onTapGesture(count: 2) { show(slide.states.first ?? slide.id) }
                    .onTapGesture { click(slide.id, in: slides) }
                    .draggable(slide.id)
                    .dropDestination(for: String.self) { dropped, _ in drop(dropped, on: slide.id, in: slides) }
                }
            }
            .padding()
        }
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(keys: [.leftArrow, .rightArrow]) { press in
            let by = press.key == .leftArrow ? -1 : 1
            if press.modifiers.contains(.option) { step(by, in: slides) } else { select(by, in: slides) }
            return .handled
        }
        .onKeyPress(keys: [.home, .end]) { press in
            if let edge = press.key == .home ? slides.first : slides.last {
                picked = [edge.id]
                anchor = edge.id
            }
            return .handled
        }
        .onKeyPress(keys: [.delete, .deleteForward]) { _ in
            remove(in: slides)
            return .handled
        }
        .onKeyPress(.return) {
            guard let slide = slides.first(where: { picked.contains($0.id) }) else { return .ignored }
            show(slide.states.first ?? slide.id)
            return .handled
        }
        .onAppear { focused = true }
    }

    private func click(_ id: String, in slides: [Slide]) {
        focused = true
        let ids = slides.map(\.id)
        if Held.shift, let anchor, let a = ids.firstIndex(of: anchor), let b = ids.firstIndex(of: id) {
            picked = Set(ids[min(a, b)...max(a, b)])
        } else if Held.command {
            if picked.contains(id) { picked.remove(id) } else { picked.insert(id) }
            anchor = id
        } else {
            picked = [id]
            anchor = id
        }
    }

    /// The slide before (`by` -1) or after the one last selected, selected alone.
    private func select(_ by: Int, in slides: [Slide]) {
        let ids = slides.map(\.id)
        let at = anchor.flatMap { ids.firstIndex(of: $0) } ?? (by < 0 ? ids.count : -1)
        guard ids.indices.contains(at + by) else { return }
        picked = [ids[at + by]]
        anchor = ids[at + by]
    }

    /// The slides selected, in the deck's order.
    private func selected(in slides: [Slide]) -> [String] {
        slides.map(\.id).filter(picked.contains)
    }

    /// Slides dropped on `target`: the slides selected where the one dragged is one of them, else it
    /// alone; before `target` where they come after it, after it where they come before.
    private func drop(_ dropped: [String], on target: String, in slides: [Slide]) -> Bool {
        let ids = slides.map(\.id)
        guard let dragged = dropped.first, ids.contains(dragged) else { return false }
        let moving = picked.contains(dragged) ? selected(in: slides) : [dragged]
        guard !moving.contains(target), let from = ids.firstIndex(of: moving[0]), let to = ids.firstIndex(of: target)
        else { return false }
        make(Restaging.moveSlides(moving, before: to < from, target), moving)
        return true
    }

    /// The slides selected a place before (`by` -1) or after, past the slide beside them.
    private func step(_ by: Int, in slides: [Slide]) {
        let ids = slides.map(\.id)
        let moving = selected(in: slides)
        guard let first = moving.first, let last = moving.last, let a = ids.firstIndex(of: first),
            let b = ids.firstIndex(of: last)
        else { return }
        let beside = by < 0 ? a - 1 : b + 1
        guard ids.indices.contains(beside), !picked.contains(ids[beside]) else { return }
        make(Restaging.moveSlides(moving, before: by < 0, ids[beside]), moving)
    }

    /// The slides selected taken out, the slide after them selected, or the one before; a deck
    /// keeps at least one.
    private func remove(in slides: [Slide]) {
        let ids = slides.map(\.id)
        let going = selected(in: slides)
        guard let last = going.last, going.count < ids.count else { return }
        let after = ids.drop { $0 != last }.dropFirst().first { !picked.contains($0) }
        let next = after ?? ids.last { !picked.contains($0) }
        make(Restaging.removeSlides(going), next.map { [$0] } ?? [])
    }
}

/// A slide drawn at its last state, its number and id, and how many steps it has.
private struct SlideCell: View {
    let editor: DeckEditor
    let slide: Slide
    let number: Int
    let picked: Bool
    let revision: Int
    @State private var drawing: CGImage?
    @Environment(\.displayScale) private var scale

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Group {
                if let drawing {
                    Image(decorative: drawing, scale: scale).resizable().aspectRatio(contentMode: .fit)
                } else {
                    Rectangle().fill(.quaternary).aspectRatio(16 / 9, contentMode: .fit)
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .overlay {
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(picked ? Color.accentColor : Color.secondary.opacity(0.3), lineWidth: picked ? 3 : 1)
            }
            HStack(spacing: 6) {
                Text("\(number)").monospacedDigit().foregroundStyle(.secondary)
                Text(slide.id).lineLimit(1)
                Spacer(minLength: 0)
                if slide.states.count > 1 {
                    Text("\(slide.states.count) steps").foregroundStyle(.secondary)
                }
            }
            .font(.caption)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(picked ? .isSelected : [])
        .task(id: "\(slide.last)\u{1f}\(revision)") {
            drawing = editor.drawing(slide.last, width: Int(320 * scale))
        }
    }
}
