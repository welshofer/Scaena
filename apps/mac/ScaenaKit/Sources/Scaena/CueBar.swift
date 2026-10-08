import ScaenaKit
import SwiftUI

/// The cue of the state shown, under the canvas (PLAN 3.4, as the browser's, PLAN 2.44): Play,
/// a ruler that scrubs it, and a bar for its transition and each motion, where the timeline
/// places them.
struct CueBar: View {
    let editor: DeckEditor
    let state: String
    @Binding var playhead: Playhead
    @State private var cue: Cue?

    var body: some View {
        let span = cue?.span ?? 0
        HStack(spacing: 10) {
            Button(action: toggle) {
                Image(systemName: playhead.playing ? "pause.fill" : "play.fill").frame(width: 16)
            }
            .buttonStyle(.borderless)
            .help(playhead.playing ? "Pause the cue" : "Play the cue")
            .disabled(span <= 0)
            VStack(spacing: 3) {
                Slider(value: scrubbed(span), in: 0...max(span, 1))
                    .controlSize(.small)
                    .disabled(span <= 0)
                if let cue, span > 0 {
                    Bars(cue: cue, span: span)
                }
            }
            Text(time(span)).font(.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 96, alignment: .trailing)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .task(id: "\(state)\u{1f}\(editor.revision)") {
            cue = try? editor.session.cue(state: state)
        }
    }

    /// The ruler's place: the playhead's time, the cue's end at rest. Scrubbed, the cue holds.
    private func scrubbed(_ span: Double) -> Binding<Double> {
        Binding(
            get: { playhead.ms.isFinite ? min(max(playhead.ms, 0), span) : span },
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
        guard span > 0 else { return "no cue" }
        let at = playhead.ms.isFinite ? min(playhead.ms, span) : span
        return "\(Int(at.rounded())) / \(Int(span.rounded())) ms"
    }
}

/// A bar for a cue's transition, then one for each motion, across the cue's span.
private struct Bars: View {
    let cue: Cue
    let span: Double

    private struct Bar {
        let name: String
        let from: Double
        let to: Double
        let transition: Bool
    }

    private var bars: [Bar] {
        var bars: [Bar] = []
        if cue.transition.duration > 0 {
            bars.append(Bar(name: "transition", from: 0, to: cue.transition.duration, transition: true))
        }
        for motion in cue.motions {
            let from = motion.moving.first ?? motion.start
            let to = motion.moving.last ?? motion.end
            bars.append(Bar(name: "\(motion.node) \(motion.motion)", from: from, to: to, transition: false))
        }
        return bars
    }

    var body: some View {
        let bars = self.bars
        GeometryReader { geometry in
            let width = geometry.size.width
            ZStack(alignment: .topLeading) {
                ForEach(bars.indices, id: \.self) { i in
                    let bar = bars[i]
                    let x = CGFloat(bar.from / span) * width
                    let w = max(CGFloat((bar.to - bar.from) / span) * width, 2)
                    Capsule()
                        .fill(bar.transition ? Color.secondary : Color.accentColor)
                        .frame(width: w, height: 3)
                        .offset(x: x, y: CGFloat(i) * 5)
                        .help("\(bar.name): \(Int(bar.from))–\(Int(bar.to)) ms")
                }
            }
        }
        .frame(height: CGFloat(max(bars.count, 1)) * 5)
    }
}
