import ScaenaKit
import SwiftUI

/// The cue of the state shown, under the canvas (PLAN 3.4, 3.14, as the browser's, PLAN 2.44):
/// Play and the time over the names, a ruler of seconds that scrubs it, and a lane for its
/// transition and each motion, where the timeline places them, each named by its object and what it
/// does, the names given room to be read. A motion's bar dragged moves its start,
/// and its end its duration; the transition's end moves its duration; each one patch, to a whole
/// ten of ms, which the engine places again. A spring's end does not move: it lasts as long as it
/// takes to settle. The canvas and the cue select together: the bars of what the canvas selects,
/// and of what that holds, stand out and the other motions' step back, and a click on a lane or its
/// name selects its object. Add Motion adds one of the theme's presets on the node selected, as it
/// enters or for emphasis, or on a node that leaves, as it leaves.
struct CueBar: View {
    let editor: DeckEditor
    let state: String
    /// What the canvas selects, the first first: whose bars stand out, and what Add Motion offers
    /// motions for.
    let selection: [String]
    @Binding var playhead: Playhead
    /// Make `ops`, one step to undo, the status saying `said`; with no ops, only say it.
    let make: ([JSONValue], String) -> Void
    /// Select a node on the canvas.
    let select: (String) -> Void
    @State private var cue: Cue?
    @State private var offers: [MotionOffer] = []
    /// Each motion's object, as a person names it.
    @State private var names: [String: String] = [:]
    /// Each object Add Motion offers a motion for, as a person names it, where it shows.
    @State private var offered: [String: Named] = [:]
    /// The bars that stand out, by id.
    @State private var chosen: Set<String> = []
    /// The bar dragged, which part of it, and how far, ms.
    @State private var dragging: Lane.Dragging?

    /// How wide the names stand: room for an object's words and what it does, as an animation
    /// pane's list gives them, the lanes the rest.
    private static let nameWidth: CGFloat = 190

    var body: some View {
        let span = cue?.span ?? 0
        let scale = lanes(cue)
        Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 3) {
            GridRow {
                // Play, Add Motion, and the time, over the names, so the lanes have the width.
                HStack(spacing: 8) {
                    Button(action: toggle) {
                        Image(systemName: playhead.playing ? "pause.fill" : "play.fill").frame(width: 16)
                    }
                    .buttonStyle(.borderless)
                    .help(playhead.playing ? "Pause the cue" : "Play the cue")
                    .disabled(span <= 0)
                    adding
                    Spacer(minLength: 4)
                    Text(time(span))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .fixedSize()
                }
                .frame(width: Self.nameWidth, alignment: .leading)
                VStack(spacing: 1) {
                    Ruler(scale: scale)
                    Slider(value: scrubbed(span, scale), in: 0...scale)
                        .controlSize(.small)
                }
            }
            if let cue {
                ForEach(cue.bars) { bar in
                    let stands = chosen.contains(bar.id)
                    GridRow {
                        label(bar, chosen: stands)
                        Lane(
                            bar: bar, named: named(bar), scale: scale, chosen: stands,
                            dimmed: !stands && !chosen.isEmpty && bar.node != nil, dragging: $dragging, time: timed,
                            select: select)
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .task(id: "\(state)\u{1f}\(editor.revision)") {
            let session = editor.session
            cue = try? session.cue(state: state)
            names = Self.naming(cue, in: state, session: session)
        }
        .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(selection.joined(separator: "\u{1f}"))") {
            let session = editor.session
            offers = (try? session.motionOffers(state: state, node: selection.first)) ?? []
            offered = Self.naming(offers, in: state, states: editor.slots.map(\.state), session: session)
            let boxes = (try? session.boxes(state: state)) ?? []
            chosen = (try? session.cue(state: state))?.chosen(selection, boxes: boxes) ?? []
        }
    }

    /// Add Motion: the theme's presets, for each motion the cue may add, each by its name and
    /// under its object's, as the browser's menu says them (PLAN 3.18).
    private var adding: some View {
        Menu {
            ForEach(offers) { offer in
                Section("\(offered[offer.node]?.name ?? Words.kind(nil)), \(Self.verb(offer.motion))") {
                    ForEach(offer.presets, id: \.self) { preset in
                        Button(Words.build(preset, as: offer.motion)) { add(preset, as: offer) }
                    }
                }
            }
        } label: {
            Image(systemName: "plus.circle")
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(offers.isEmpty)
        .help(
            offers.isEmpty
                ? "Select a node on the canvas to add a motion to it" : "Add a motion: one of the theme's presets")
    }

    /// A lane's name: its object and what the motion does, the transition by its kind; how it splits
    /// is the lane's to say, to the pointer and VoiceOver, so the object's name has the room. A click
    /// on it selects the object, as a click on the lane does.
    private func label(_ bar: Cue.Bar, chosen: Bool) -> some View {
        HStack(spacing: 4) {
            if let node = bar.node {
                Text(names[node] ?? Words.kind(nil)).lineLimit(1).truncationMode(.tail)
                Text(Words.motion(bar.motion)).foregroundStyle(.secondary).lineLimit(1).fixedSize()
            } else {
                Text("Transition").lineLimit(1)
            }
        }
        .font(.caption2.weight(chosen ? .semibold : .regular))
        .foregroundStyle(chosen ? HierarchicalShapeStyle.primary : .secondary)
        .frame(width: Self.nameWidth, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture { if let node = bar.node { select(node) } }
        // The whole name, where the column cuts a long one short.
        .help(named(bar))
        // The lane says it, to VoiceOver.
        .accessibilityHidden(true)
    }

    /// What a lane is called, in a sentence's words: its object and what the motion does.
    private func named(_ bar: Cue.Bar) -> String {
        guard let node = bar.node else { return "Transition" }
        return "\(names[node] ?? Words.kind(nil)), \(Words.motion(bar.motion, split: bar.split).lowercased())"
    }

    /// Each motion's object named as the layers name it: its words, else its kind. A node that
    /// leaves is named by its words in the state it leaves.
    private static func naming(_ cue: Cue?, in state: String, session: ScaenaSession) -> [String: String] {
        guard let cue, !cue.motions.isEmpty else { return [:] }
        return objects(cue.motions.map(\.node), in: state, session: session).mapValues(\.name)
    }

    /// Each object an offer is for, named where it shows: in `state`, or, where it leaves, in the
    /// state before, as `states` lists them.
    private static func naming(
        _ offers: [MotionOffer], in state: String, states: [String], session: ScaenaSession
    ) -> [String: Named] {
        var names = objects(offers.filter { $0.motion != "exit" }.map(\.node), in: state, session: session)
        if let i = states.firstIndex(of: state), i > 0 {
            let gone = objects(offers.filter { $0.motion == "exit" }.map(\.node), in: states[i - 1], session: session)
            names.merge(gone) { here, _ in here }
        }
        return names
    }

    /// What a motion does, after its object's name: `as it enters`.
    private static func verb(_ motion: String) -> String {
        ["enter": "as it enters", "exit": "as it leaves"][motion] ?? "for emphasis"
    }

    /// Each of `nodes` named as the layers name it in `state`: its words, else its kind.
    private static func objects(_ nodes: [String], in state: String, session: ScaenaSession) -> [String: Named] {
        guard !nodes.isEmpty else { return [:] }
        var words: [String: String] = [:]
        for part in (try? session.reads(state: state)) ?? [] where !part.text.isEmpty {
            words[part.node] = part.text
        }
        var types: [String: String] = [:]
        func walk(_ layers: [Layer]) {
            for layer in layers {
                types[layer.node] = layer.type
                walk(layer.children)
            }
        }
        walk((try? session.layers(state: state)) ?? [])
        let kinds = (try? session.kinds(state: state)) ?? [:]
        var names: [String: Named] = [:]
        for node in nodes where names[node] == nil {
            let text = words[node] ?? (types[node] == "text" ? (try? session.text(state: state, node: node)) ?? nil : nil)
            names[node] = Named(type: types[node], words: text, kind: kinds[node])
        }
        return names
    }

    /// How many ms the lanes span: the cue, and room past it to drag a bar into, as the browser's.
    private func lanes(_ cue: Cue?) -> Double {
        let longest = max(1000, cue?.span ?? 0, cue?.transition.duration ?? 0)
        return (longest * 1.25 / 100).rounded(.up) * 100
    }

    /// The ruler's place: the playhead's time, the cue's end at rest. Scrubbed, the cue holds.
    private func scrubbed(_ span: Double, _ scale: Double) -> Binding<Double> {
        Binding(
            get: { playhead.ms.isFinite ? min(max(playhead.ms, 0), scale) : span },
            set: { playhead = Playhead(ms: $0, playing: false) })
    }

    private func toggle() {
        if playhead.playing {
            playhead = Playhead(ms: playhead.ms, playing: false)
        } else {
            playhead.playing = true
        }
    }

    private func time(_ span: Double) -> String {
        guard span > 0 else { return "No animation" }
        let at = playhead.ms.isFinite ? min(playhead.ms, span) : span
        return "\(Words.seconds(at)) / \(Words.seconds(span))"
    }

    /// `bar`'s start or end moved `by` ms: the patch that times it, made.
    private func timed(_ bar: Cue.Bar, _ part: Cue.Bar.Part, _ by: Double) {
        guard let timing = bar.timing(part, by: by, in: state) else { return }
        make(timing.patch, timing.said)
    }

    /// `preset` added as `offer`'s motion, said as the browser says it.
    private func add(_ preset: String, as offer: MotionOffer) {
        guard let patch = offer.applying(preset, in: state, states: editor.slots.map(\.state)) else {
            return make([], "It isn't on the slide before this one")
        }
        let it = offered[offer.node]?.quoted ?? "it"
        make(patch, "\(Words.build(preset, as: offer.motion)) added to \(it) \(Self.verb(offer.motion))")
    }
}

/// An object as a person names it (PLAN 3.18): its words where it has any, else its kind; and as a
/// sentence quotes it, its words in quotation marks, else `the` and its kind.
private struct Named {
    let name: String
    let quoted: String

    init(type: String?, words: String?, kind: String? = nil) {
        name = Words.node(type, words: words, kind: kind)
        quoted = name == (kind ?? Words.kind(type)) ? "the \(name.lowercased())" : "\u{201C}\(name)\u{201D}"
    }
}

/// The seconds across the lanes, as an animation pane's scale: a mark a round step apart (`Cue.ticks`)
/// and, where there is room for it, its time, over the ruler that scrubs the cue.
private struct Ruler: View {
    /// How many ms the lanes span.
    let scale: Double

    var body: some View {
        GeometryReader { geometry in
            let width = max(geometry.size.width, 1)
            ForEach(Cue.ticks(across: scale, width: Double(width)), id: \.self) { ms in
                let x = CGFloat(ms / scale) * width
                Rectangle()
                    .fill(Color.secondary.opacity(0.6))
                    .frame(width: 1, height: 4)
                    .position(x: min(x + 0.5, width - 0.5), y: 12)
                // Its time from the mark on, where it has the room before the lanes' end.
                if x <= width - 30 {
                    Text(Words.seconds(ms))
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .fixedSize()
                        .frame(width: 40, alignment: .leading)
                        .position(x: x + 22, y: 5)
                }
            }
        }
        .frame(height: 14)
        // The scrubber says the time; the marks only show it.
        .accessibilityHidden(true)
    }
}

/// A lane of the cue, across `scale` ms: its bar where the timeline places it, which a drag moves
/// the start of, and its end's handle, which a drag moves the duration of. A click on the lane
/// selects its object; its object selected, the lane stands out, and another's, it steps back.
private struct Lane: View {
    let bar: Cue.Bar
    /// What it is called: its object and what the motion does.
    let named: String
    let scale: Double
    /// Whether what the canvas selects is its object, or holds it; and whether the canvas selects
    /// another's.
    let chosen: Bool
    let dimmed: Bool
    @Binding var dragging: Dragging?
    /// A bar's start or end let go, moved by so many ms.
    let time: (Cue.Bar, Cue.Bar.Part, Double) -> Void
    let select: (String) -> Void

    struct Dragging {
        let bar: String
        let part: Cue.Bar.Part
        let by: Double
    }

    /// How tall a lane stands: on the iPad, for a finger.
    #if os(macOS)
    static let height: CGFloat = 12
    #else
    static let height: CGFloat = 22
    #endif

    var body: some View {
        GeometryReader { geometry in
            let width = max(geometry.size.width, 1)
            let moved = dragging?.bar == bar.id ? dragging : nil
            let shift = moved?.part == .delay ? moved?.by ?? 0 : 0
            let grow = moved?.part == .duration ? moved?.by ?? 0 : 0
            let x = CGFloat(max(0, bar.from + shift) / scale) * width
            let w = max(CGFloat(max(0, bar.to - bar.from + grow) / scale) * width, 3)
            let perPoint = scale / Double(width)
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(chosen ? Color.accentColor.opacity(0.14) : Color.clear)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: pick)
                Capsule()
                    .fill(fill)
                    .overlay(Capsule().strokeBorder(Color.primary.opacity(chosen ? 0.55 : 0), lineWidth: 1))
                    .frame(width: w, height: chosen ? 8 : 6)
                    .offset(x: x)
                    .onTapGesture(perform: pick)
                    .gesture(drag(.delay, perPoint: perPoint), including: bar.node == nil ? .none : .all)
                if !bar.sprung {
                    Rectangle()
                        .fill(Color.primary.opacity(0.55))
                        .frame(width: 4, height: 10)
                        .offset(x: x + w - 2)
                        .gesture(drag(.duration, perPoint: perPoint))
                }
            }
            .frame(width: width, height: geometry.size.height, alignment: .leading)
        }
        .frame(height: Self.height)
        .help(said)
        .accessibilityElement()
        .accessibilityLabel(named)
        .accessibilityValue("\(Words.seconds(bar.from)) to \(Words.seconds(bar.to))")
        .accessibilityAddTraits(chosen ? [.isButton, .isSelected] : [.isButton])
        .accessibilityAction { pick() }
        .accessibilityIdentifier("cue-\(bar.id)")
    }

    /// The bar's color: the transition's quiet, a motion's the accent, stepping back while another's
    /// object is selected.
    private var fill: Color {
        guard bar.node != nil else { return .secondary }
        return dimmed ? Color.accentColor.opacity(0.35) : .accentColor
    }

    /// What a pointer over the lane is told: what it is, where the timeline places it, and what a
    /// click and a drag do.
    private var said: String {
        guard bar.node != nil else {
            return "\(named): \(Words.seconds(bar.duration)). Drag its end to change how long it lasts."
        }
        let when = "\(Words.seconds(bar.from))–\(Words.seconds(bar.to)), after \(Words.seconds(bar.delay))"
        return "\(named): \(when). Click to select it; drag to change when it starts."
    }

    private func pick() {
        if let node = bar.node { select(node) }
    }

    /// A drag of the bar's `part`: the bar follows it, and where it is let go is one patch.
    private func drag(_ part: Cue.Bar.Part, perPoint: Double) -> some Gesture {
        DragGesture(minimumDistance: 2)
            .onChanged { value in
                dragging = Dragging(bar: bar.id, part: part, by: Double(value.translation.width) * perPoint)
            }
            .onEnded { value in
                dragging = nil
                time(bar, part, Double(value.translation.width) * perPoint)
            }
    }
}
