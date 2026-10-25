import ScaenaKit
import SwiftUI

/// The light table (PLAN 2.97, 3.14), in place of the canvas and the slides down the side, as a
/// presentation app's: every slide of the deck in a grid that fills the window, each drawn at its
/// last state at rest, as the PDF draws it, to reorder, copy, and take out across a large deck. The
/// slider under the grid, or a pinch, makes the slides larger or smaller, as many to a row as fit,
/// and the size is kept for the next time. A click selects a slide, Shift+click every slide from the
/// one last clicked, ⌘-click adds one or takes it away, as does a tap with the toolbar's Select held
/// (lift-up FUNC-4); ← and → select the slide before or after,
/// Home and End the first and the last. The slides selected, dragged onto another, go before it
/// where they come after it and after it where they come before; ⌥← and ⌥→ move them a place; ⌘D
/// (the Node menu's Duplicate) copies them, each just after itself; Delete takes them out. Return,
/// or a double click, shows a slide's first state on the canvas. Each is one patch, and every
/// other state shows what it showed.
struct LightTable: View {
    let editor: DeckEditor
    /// The slides selected.
    @Binding var picked: Set<String>
    /// Show `state` on the canvas, the table put away.
    let show: (String) -> Void
    /// Make `ops`, one step to undo, then select the slides `then`.
    let make: ([JSONValue], [String]) -> Void
    /// What a touch holds in a keyboard's place (lift-up FUNC-4): Select held, a tap adds a slide or
    /// takes it out, as ⌘ and a click do.
    var touch: TouchHeld? = nil
    /// The slide last clicked: where Shift+click selects from.
    @State private var anchor: String?
    @FocusState private var focused: Bool
    /// How wide each slide is drawn, in points: the slider's, kept between windows and launches.
    @AppStorage("lightTableSlideWidth") private var slideWidth = LightTable.ideal
    /// The slide width a pinch began from, while one goes on.
    @State private var pinchedFrom: Double?

    /// The narrowest a slide is drawn, the widest, and how wide at first, in points.
    static let least = 120.0
    static let most = 520.0
    static let ideal = 240.0
    /// The room between slides, and around them, in points.
    private static let spacing: CGFloat = 20
    private static let margin: CGFloat = 24

    var body: some View {
        let slides = Slide.of(editor.slots)
        VStack(spacing: 0) {
            // The grid takes all the room the window gives it, as many slides to a row as fit.
            GeometryReader { room in
                let columns = Self.columns(across: room.size.width, slide: slideWidth)
                let cell = Self.cellWidth(across: room.size.width, columns: columns)
                ScrollView {
                    LazyVGrid(
                        columns: Array(repeating: GridItem(.flexible(), spacing: Self.spacing, alignment: .top), count: columns),
                        spacing: Self.spacing
                    ) {
                        ForEach(Array(slides.enumerated()), id: \.element.id) { n, slide in
                            SlideCell(
                                editor: editor, slide: slide, number: n + 1, picked: picked.contains(slide.id),
                                revision: editor.revision, width: cell,
                                headline: editor.slots.first { $0.state == slide.last }?.headline
                            )
                            .accessibilityActions { actions(slide, in: slides) }
                            .onTapGesture(count: 2) { show(slide.states.first ?? slide.id) }
                            .onTapGesture { click(slide.id, in: slides) }
                            .draggable(slide.id)
                            .dropDestination(for: String.self) { dropped, _ in drop(dropped, on: slide.id, in: slides) }
                        }
                    }
                    .padding(Self.margin)
                }
            }
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { pinch in
                        let from = pinchedFrom ?? slideWidth
                        pinchedFrom = from
                        slideWidth = min(max(from * Double(pinch.magnification), Self.least), Self.most)
                    }
                    .onEnded { _ in pinchedFrom = nil }
            )
            Divider()
            sizing
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

    /// The slider that sizes the slides, from a small slide to a large one, at the foot of the
    /// table, as a presentation app's light table has it.
    private var sizing: some View {
        HStack(spacing: 8) {
            Spacer()
            Image(systemName: "rectangle").imageScale(.small).foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Slider(value: $slideWidth, in: Self.least...Self.most)
                .frame(width: 180)
                .accessibilityLabel("Slide Size")
                .accessibilityIdentifier("light-table-size")
            Image(systemName: "rectangle").imageScale(.large).foregroundStyle(.secondary)
                .accessibilityHidden(true)
        }
        .controlSize(.small)
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .background(.bar)
    }

    /// How many slides fit across `width` points, each at least `slide` wide: one at least.
    static func columns(across width: CGFloat, slide: Double) -> Int {
        max(1, Int(Double(width - 2 * margin + spacing) / (slide + Double(spacing))))
    }

    /// How wide each of `columns` slides is across `width` points: the row filled.
    static func cellWidth(across width: CGFloat, columns: Int) -> CGFloat {
        max((width - 2 * margin - CGFloat(columns - 1) * spacing) / CGFloat(columns), 1)
    }

    private func click(_ id: String, in slides: [Slide]) {
        focused = true
        let ids = slides.map(\.id)
        if Held.shift, let anchor, let a = ids.firstIndex(of: anchor), let b = ids.firstIndex(of: id) {
            picked = Set(ids[min(a, b)...max(a, b)])
        } else if Held.command || touch?.selecting == true {
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

    /// What VoiceOver offers on a slide (lift-up 10-24b USE-4), each what its key does to it alone:
    /// moved a place, copied, taken out, or shown on the canvas.
    @ViewBuilder private func actions(_ slide: Slide, in slides: [Slide]) -> some View {
        let ids = slides.map(\.id)
        if let i = ids.firstIndex(of: slide.id) {
            if i > 0 {
                Button("Move Before") { make(Restaging.moveSlides([slide.id], before: true, ids[i - 1]), [slide.id]) }
            }
            if i + 1 < ids.count {
                Button("Move After") { make(Restaging.moveSlides([slide.id], before: false, ids[i + 1]), [slide.id]) }
            }
        }
        Button("Duplicate Slide") { make(Restaging.duplicateSlides([slide.id]), [slide.id]) }
        if ids.count > 1 {
            Button("Delete Slide") {
                let after = ids.drop { $0 != slide.id }.dropFirst().first ?? ids.last { $0 != slide.id }
                make(Restaging.removeSlides([slide.id]), after.map { [$0] } ?? [])
            }
        }
        Button("Show on the Canvas") { show(slide.states.first ?? slide.id) }
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

/// A slide drawn at its last state, under it its number and how many steps it has.
private struct SlideCell: View {
    let editor: DeckEditor
    let slide: Slide
    let number: Int
    let picked: Bool
    let revision: Int
    /// How wide it stands, in points.
    let width: CGFloat
    /// The words that head it as it is drawn, which VoiceOver reads it by (lift-up 10-24b USE-4).
    let headline: String?
    @State private var drawing: CGImage?
    @Environment(\.displayScale) private var scale

    /// How wide it is drawn, in pixels: its width, rounded up to the next 160 points, so a slider
    /// dragged draws the slides again only now and then, the drawing scaled between.
    private var drawn: Int {
        Int((width / 160).rounded(.up) * 160 * scale)
    }

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
            // Its number under it, as a presentation app's light table has it, and its steps.
            HStack(spacing: 6) {
                Text("\(number)").monospacedDigit()
                if slide.states.count > 1 {
                    Text("· \(slide.states.count) steps")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(SlideReading.cell(number: number, headline: headline))
        .accessibilityValue(SlideReading.steps(slide.states.count))
        .accessibilityIdentifier("slide-\(slide.id)")
        .accessibilityAddTraits(picked ? .isSelected : [])
        .task(id: "\(slide.last)\u{1f}\(revision)\u{1f}\(drawn)") {
            drawing = await editor.drawingAway(slide.last, width: drawn)
        }
    }
}
