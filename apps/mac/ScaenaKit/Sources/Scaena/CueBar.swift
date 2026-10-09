import ScaenaKit
import SwiftUI

/// The cue of the state shown, under the canvas (PLAN 3.4, 3.14, as the browser's, PLAN 2.44):
/// Play, a ruler that scrubs it, and a lane for its transition and each motion, where the timeline
/// places them. A motion's bar dragged moves its start, and its end its duration; the transition's
/// end moves its duration; each one patch, to a whole ten of ms, which the engine places again. A
/// spring's end does not move: it lasts as long as it takes to settle. A press on a motion's bar
/// selects its node. Add Motion adds one of the theme's presets on the node selected, as it enters
/// or for emphasis, or on a node that leaves, as it leaves.
struct CueBar: View {
    let editor: DeckEditor
    let state: String
    /// The node the canvas selects: what Add Motion offers motions for.
    let node: String?
    @Binding var playhead: Playhead
    /// Make `ops`, one step to undo, the status saying `said`; with no ops, only say it.
    let make: ([JSONValue], String) -> Void
    /// Select a node on the canvas.
    let select: (String) -> Void
    @State private var cue: Cue?
    @State private var offers: [MotionOffer] = []

    var body: some View {
        let span = cue?.span ?? 0
        let scale = lanes(cue)
        HStack(spacing: 10) {
            Button(action: toggle) {
                Image(systemName: playhead.playing ? "pause.fill" : "play.fill").frame(width: 16)
            }
            .buttonStyle(.borderless)
            .help(playhead.playing ? "Pause the cue" : "Play the cue")
            .disabled(span <= 0)
            adding
            VStack(spacing: 3) {
                Slider(value: scrubbed(span, scale), in: 0...scale)
                    .controlSize(.small)
                if let cue {
                    Lanes(bars: cue.bars, scale: scale, time: timed, select: select)
                }
            }
            Text(time(span)).font(.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 96, alignment: .trailing)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(node ?? "")") {
            cue = try? editor.session.cue(state: state)
            offers = (try? editor.session.motionOffers(state: state, node: node)) ?? []
        }
    }

    /// Add Motion: the theme's presets, for each motion the cue may add.
    private var adding: some View {
        Menu {
            ForEach(offers) { offer in
                Section(offer.label) {
                    ForEach(offer.presets, id: \.self) { preset in
                        Button(preset) { add(preset, as: offer) }
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

    /// `preset` added as `offer`'s motion.
    private func add(_ preset: String, as offer: MotionOffer) {
        let verb = ["enter": "as it enters", "exit": "as it leaves"][offer.motion] ?? "for emphasis"
        guard let patch = offer.applying(preset, in: state, states: editor.slots.map(\.state)) else {
            return make([], "It isn't on the slide before this one")
        }
        make(patch, "\(Words.phrase(preset)) added \(verb)")
    }
}

/// A lane for the cue's transition, then one for each motion, across `scale` ms: a bar dragged
/// moves its start, its end handle its duration, and a press on a motion's bar selects its node.
private struct Lanes: View {
    let bars: [Cue.Bar]
    let scale: Double
    /// A bar's start or end let go, moved by so many ms.
    let time: (Cue.Bar, Cue.Bar.Part, Double) -> Void
    let select: (String) -> Void
    /// The bar dragged, which part of it, and how far, ms.
    @State private var dragging: Dragging?

    private struct Dragging {
        let bar: String
        let part: Cue.Bar.Part
        let by: Double
    }

    var body: some View {
        GeometryReader { geometry in
            let width = max(geometry.size.width, 1)
            VStack(alignment: .leading, spacing: 3) {
                ForEach(bars) { bar in
                    lane(bar, width: width)
                }
            }
        }
        .frame(height: CGFloat(bars.count) * 11)
    }

    private func lane(_ bar: Cue.Bar, width: CGFloat) -> some View {
        let moved = dragging?.bar == bar.id ? dragging : nil
        let shift = moved?.part == .delay ? moved?.by ?? 0 : 0
        let grow = moved?.part == .duration ? moved?.by ?? 0 : 0
        let x = CGFloat(max(0, bar.from + shift) / scale) * width
        let w = max(CGFloat(max(0, bar.to - bar.from + grow) / scale) * width, 3)
        let perPoint = scale / Double(width)
        return ZStack(alignment: .leading) {
            Capsule()
                .fill(bar.node == nil ? Color.secondary : Color.accentColor)
                .frame(width: w, height: 6)
                .offset(x: x)
                .onTapGesture { if let node = bar.node { select(node) } }
                .gesture(drag(bar, .delay, perPoint: perPoint), including: bar.node == nil ? .none : .all)
            if !bar.sprung {
                Rectangle()
                    .fill(Color.primary.opacity(0.55))
                    .frame(width: 4, height: 8)
                    .offset(x: x + w - 2)
                    .gesture(drag(bar, .duration, perPoint: perPoint))
            }
        }
        .frame(width: width, height: 8, alignment: .leading)
        .help(said(bar))
    }

    /// What a pointer over `bar` is told: what it is, and where the timeline places it.
    private func said(_ bar: Cue.Bar) -> String {
        let waits = bar.node == nil ? "" : ", after \(Words.seconds(bar.delay))"
        return "\(bar.label): \(Words.seconds(bar.from))–\(Words.seconds(bar.to))\(waits)"
    }

    /// A drag of `bar`'s `part`: the bar follows it, and where it is let go is one patch.
    private func drag(_ bar: Cue.Bar, _ part: Cue.Bar.Part, perPoint: Double) -> some Gesture {
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
