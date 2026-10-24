import ScaenaKit
import SwiftUI

/// The deck's story (PLAN G.9, as the browser's Story tab): how well it tells it, as Vzestup measures
/// a talk. Its score, and each measure with what it says; the arc, its curve, the intensity each
/// slide declares on it, and what its words read, a click on a slide's place showing it; and what to
/// look at, a row for each slide in the order they stand, its headline beside it: what health finds
/// there, each fix one step to undo, and its questions, which the assistant takes into its box. Read
/// again as the deck changes.
struct StoryPanel: View {
    let editor: DeckEditor
    let edits: PanelEdits
    @State private var health: Health?
    @State private var why: String?

    var body: some View {
        List {
            if let health {
                Section {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("\(health.score)")
                            .font(.largeTitle.monospacedDigit())
                            .foregroundStyle(Self.tint(health.score))
                            .accessibilityLabel("Narrative health \(health.score) of 100")
                        Text(health.arc.map { "of 100, written to the \($0) arc" } ?? "of 100, no arc named")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(Self.measures(health), id: \.name) { measure in
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Text(measure.name)
                                .foregroundStyle(.secondary)
                                .frame(width: 70, alignment: .leading)
                            Text(measure.value.score.map(String.init) ?? "—")
                                .monospacedDigit()
                                .foregroundStyle(measure.value.score.map(Self.tint) ?? .secondary)
                                .frame(width: 28, alignment: .trailing)
                            Text(measure.value.says).font(.callout)
                        }
                    }
                }
                Section("The Arc") {
                    ArcGraph(slides: health.slides) { edits.go($0) }
                        .frame(height: 110)
                    ArcLegend()
                }
                Section("What to Look At") {
                    if health.findings.isEmpty {
                        Text("Nothing to look at: each slide tells its part.").foregroundStyle(.secondary)
                    }
                    ForEach(Self.bySlide(health.findings), id: \.first?.slide) { run in
                        row(run, health)
                    }
                }
            } else if let why {
                Text(why).foregroundStyle(.secondary)
            }
        }
        .task(id: editor.revision) { await read() }
    }

    /// Read at once as the panel shows, and again once edits stop, as every state is linted then
    /// (PLAN 4.13), away from the window, which answers meanwhile (lift-up narrative PERF-1): an
    /// edit made before the pause ends cancels this read, and one made while it is read drops it.
    private func read() async {
        if health != nil || why != nil {
            try? await Task.sleep(for: .milliseconds(700))
        }
        guard !Task.isCancelled else { return }
        do {
            guard let read = try await editor.health(), !Task.isCancelled else { return }
            health = read
            why = nil
        } catch {
            health = nil
            why = "Its story couldn't be read. \(Why.said(error))"
        }
    }

    /// A slide's row: its number, which shows it, and its headline; what health finds there; each
    /// fix it offers; and its questions, asked together.
    private func row(_ run: [Health.Finding], _ health: Health) -> some View {
        let first = run[0]
        let title = health.slides.indices.contains(first.slide - 1) ? health.slides[first.slide - 1].title : ""
        return VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Button("Slide \(first.slide)") { edits.go(first.state) }
                    .buttonStyle(.borderless)
                    .foregroundStyle(Self.tint(severity: Self.worst(run)))
                Text(title)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            ForEach(run) { finding in
                Text(Self.sentence(finding)).font(.callout)
            }
            HStack(spacing: 8) {
                ForEach(run.filter { $0.fix != nil }) { finding in
                    if let fix = finding.fix {
                        Button(fix.says) { edits.patch("Fix Slide \(finding.slide)", fix.patch) }
                    }
                }
                Button("Ask the Assistant") { edits.ask(run.map(\.ask).joined(separator: " ")) }
            }
            .controlSize(.small)
        }
        .padding(.vertical, 2)
    }

    /// The findings, a run for each slide, in the order the slides stand (as health gives them).
    static func bySlide(_ findings: [Health.Finding]) -> [[Health.Finding]] {
        var runs: [[Health.Finding]] = []
        for finding in findings {
            if runs.last?.first?.slide == finding.slide {
                runs[runs.count - 1].append(finding)
            } else {
                runs.append([finding])
            }
        }
        return runs
    }

    /// A finding as its slide's row says it: "Slide 4 runs hotter…" as "Runs hotter…", since the
    /// row names its slide.
    static func sentence(_ finding: Health.Finding) -> String {
        let named = "Slide \(finding.slide) "
        guard finding.says.hasPrefix(named) else { return finding.says }
        let rest = finding.says.dropFirst(named.count)
        return rest.prefix(1).uppercased() + rest.dropFirst()
    }

    /// The gravest of a run's findings.
    private static func worst(_ run: [Health.Finding]) -> String {
        let order = ["info", "warning", "error"]
        return run.map(\.severity).max { (order.firstIndex(of: $0) ?? 0) < (order.firstIndex(of: $1) ?? 0) } ?? "info"
    }

    private struct Named {
        let name: String
        let value: Health.Measure
    }

    private static func measures(_ h: Health) -> [Named] {
        [
            Named(name: "Pacing", value: h.pacing), Named(name: "Balance", value: h.balance),
            Named(name: "Coherence", value: h.coherence), Named(name: "Arc", value: h.fidelity),
            Named(name: "Intensity", value: h.intensity), Named(name: "Tone", value: h.tone),
        ]
    }

    /// How a score reads: good from 80, fair from 60.
    private static func tint(_ score: Int) -> Color {
        score >= 80 ? .green : score >= 60 ? .orange : .red
    }

    private static func tint(severity: String) -> Color {
        switch severity {
        case "error": .red
        case "warning": .orange
        default: .accentColor
        }
    }
}

/// The arc as the story panel draws it: the curve of the arc the deck was written to, dashed; the
/// intensity each slide declares on it, a line; and what its words read, a ring. A slide that bends
/// the curve is marked orange, one that drifted from its beat red. A click shows the slide nearest.
struct ArcGraph: View {
    let slides: [Health.Read]
    let go: (String) -> Void

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let n = slides.count
            let step = n > 1 ? (size.width - 12) / CGFloat(n - 1) : 0
            let x = { (i: Int) -> CGFloat in n > 1 ? 6 + CGFloat(i) * step : size.width / 2 }
            let y = { (v: Double) -> CGFloat in size.height - 6 - CGFloat(v) * (size.height - 12) }
            ZStack {
                Path { path in
                    for (i, slide) in slides.enumerated() {
                        let point = CGPoint(x: x(i), y: y(slide.curve))
                        if i == 0 { path.move(to: point) } else { path.addLine(to: point) }
                    }
                }
                .stroke(.secondary, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                Path { path in
                    var started = false
                    for (i, slide) in slides.enumerated() {
                        guard let declared = slide.declared else { continue }
                        let point = CGPoint(x: x(i), y: y(declared))
                        if started { path.addLine(to: point) } else { path.move(to: point) }
                        started = true
                    }
                }
                .stroke(Color.accentColor, lineWidth: 1.5)
                ForEach(Array(slides.enumerated()), id: \.offset) { i, slide in
                    Circle()
                        .fill(Self.color(slide.standing))
                        .frame(width: 7, height: 7)
                        .position(x: x(i), y: y(slide.declared ?? slide.curve))
                    if let read = slide.read {
                        Circle()
                            .stroke(.primary, lineWidth: 1)
                            .frame(width: 5, height: 5)
                            .position(x: x(i), y: y(read))
                    }
                }
            }
            .contentShape(Rectangle())
            .onTapGesture { point in
                guard n > 0 else { return }
                let i = n > 1 ? Int(((point.x - 6) / step).rounded()) : 0
                go(slides[min(max(i, 0), n - 1)].state)
            }
        }
        .accessibilityElement()
        .accessibilityLabel("The intensity each slide declares against the curve of its arc")
    }

    static func color(_ standing: String) -> Color {
        switch standing {
        case "bends": .orange
        case "drifted": .red
        default: .accentColor
        }
    }
}

/// What the arc's marks mean, under it.
struct ArcLegend: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 4) {
                Rectangle().fill(Color.accentColor).frame(width: 14, height: 1.5)
                Text("As written")
                Circle().stroke(.primary, lineWidth: 1).frame(width: 5, height: 5).padding(.leading, 6)
                Text("As its words read")
                Rectangle()
                    .stroke(.secondary, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                    .frame(width: 14, height: 1)
                    .padding(.leading, 6)
                Text("The arc's curve")
            }
            HStack(spacing: 4) {
                Circle().fill(ArcGraph.color("bends")).frame(width: 7, height: 7)
                Text("Off the curve. Higher is more intense.")
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
    }
}
