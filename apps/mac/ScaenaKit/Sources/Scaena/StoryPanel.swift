import ScaenaKit
import SwiftUI

/// The deck's story (PLAN G.9, as the browser's Story tab): how well it tells it, as Vzestup measures
/// a talk. Its score, and each measure with what it says; the big idea the settled outline names;
/// the arc, its curve, the intensity each slide declares on it, and what its words read, a click on
/// a slide's place showing it; and what to look at, what to do first open and the rest folded under
/// one "N more" counted by kind (`Health.triaged`, lift-up S1), a row for each slide, its headline
/// beside it: what health finds there, each fix one step to undo, and its questions, which the
/// assistant takes into its box. Read again as the deck changes.
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
                    // The one sentence the outline settled, above the curve it is to land on
                    // (lift-up 10-24b FUNC-1).
                    if let idea = health.bigIdea {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("The big idea").font(.caption).foregroundStyle(.secondary)
                            Text("\u{201C}\(idea)\u{201D}").italic()
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("story-big-idea")
                    }
                    ArcGraph(slides: health.slides, beats: health.beats) { edits.go($0) }
                        .frame(height: 110)
                    ArcLegend()
                }
                Section("What to Look At") {
                    if health.findings.isEmpty {
                        Text("Nothing to look at: each slide tells its part.").foregroundStyle(.secondary)
                    }
                    // What to do first, open; the rest folded under one "N more" (lift-up S1).
                    let triaged = health.triaged()
                    ForEach(Self.bySlide(triaged.open), id: \.first?.slide) { run in
                        row(run, health)
                    }
                    if !triaged.more.isEmpty {
                        DisclosureGroup(Health.moreSaid(triaged.more)) {
                            ForEach(Self.bySlide(triaged.more), id: \.first?.slide) { run in
                                row(run, health)
                            }
                        }
                        .accessibilityIdentifier("story-more")
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
                // The slide it is about shown first: a question begins with what the window shows,
                // which "this slide" means to the assistant (lift-up USE-5).
                Button("Ask the Assistant") {
                    edits.go(first.state)
                    edits.ask(run.map(\.ask).joined(separator: " "))
                }
            }
            .controlSize(.small)
        }
        .padding(.vertical, 2)
    }

    /// The findings a row for each slide, in the order they come: a slide's findings together,
    /// where its first stands.
    static func bySlide(_ findings: [Health.Finding]) -> [[Health.Finding]] {
        var runs: [[Health.Finding]] = []
        for finding in findings {
            if let i = runs.firstIndex(where: { $0.first?.slide == finding.slide }) {
                runs[i].append(finding)
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

/// The arc as the story panel draws it: the arc's beats as bands under it, each named where it has
/// room; the curve of the arc the deck was written to, dashed; the intensity each slide declares
/// on it, a line; what its words read, a ring; and the tone each declares, a thinner line from
/// grave at the foot to hopeful at the top, with what its words read, a square (lift-up ATTR-4). A
/// slide that bends the curve is marked orange, one that drifted from its beat red. A click shows
/// the slide nearest.
struct ArcGraph: View {
    let slides: [Health.Read]
    let beats: [String]
    let go: (String) -> Void

    /// Each run of slides written for one beat of the arc: the beat, and its first and last slide.
    static func runs(_ slides: [Health.Read]) -> [(part: Int, from: Int, to: Int)] {
        var runs: [(part: Int, from: Int, to: Int)] = []
        for (i, slide) in slides.enumerated() {
            guard let part = slide.part else { continue }
            if let last = runs.last, last.part == part, last.to == i - 1 {
                runs[runs.count - 1].to = i
            } else {
                runs.append((part, i, i))
            }
        }
        return runs
    }

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let n = slides.count
            let step = n > 1 ? (size.width - 12) / CGFloat(n - 1) : 0
            let x = { (i: Int) -> CGFloat in n > 1 ? 6 + CGFloat(i) * step : size.width / 2 }
            let y = { (v: Double) -> CGFloat in size.height - 6 - CGFloat(v) * (size.height - 12) }
            let toned = { (t: Double) -> CGFloat in y((t + 1) / 2) }
            ZStack {
                ForEach(Array(Self.runs(slides).enumerated()), id: \.offset) { _, run in
                    let half = n > 1 ? step / 2 : size.width / 2
                    let left = max(0, x(run.from) - half)
                    let right = min(size.width, x(run.to) + half)
                    let name = run.part < beats.count ? beats[run.part] : ""
                    Rectangle()
                        .fill(.secondary.opacity(run.part % 2 == 0 ? 0.14 : 0.06))
                        .frame(width: right - left, height: size.height)
                        .position(x: (left + right) / 2, y: size.height / 2)
                    Text(name)
                        .font(.system(size: 8))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .frame(width: right - left)
                        .opacity(CGFloat(name.count) * 4.6 + 6 <= right - left ? 1 : 0)
                        .position(x: (left + right) / 2, y: 6)
                        .accessibilityHidden(true)
                }
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
                Path { path in
                    var started = false
                    for (i, slide) in slides.enumerated() {
                        guard let tone = slide.tone else { continue }
                        let point = CGPoint(x: x(i), y: toned(tone))
                        if started { path.addLine(to: point) } else { path.move(to: point) }
                        started = true
                    }
                }
                .stroke(.primary.opacity(0.7), lineWidth: 0.75)
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
                    if let heard = slide.toneRead {
                        Rectangle()
                            .stroke(.primary, lineWidth: 0.75)
                            .frame(width: 3.5, height: 3.5)
                            .position(x: x(i), y: toned(heard))
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
        .accessibilityLabel("The intensity and tone each slide declares against the curve of its arc, by the arc's beats")
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
                Text("Intensity as written")
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
                Text("Off the curve")
                Circle().fill(ArcGraph.color("drifted")).frame(width: 7, height: 7).padding(.leading, 6)
                Text("Out of its beat. Higher is more intense.")
            }
            HStack(spacing: 4) {
                Rectangle().fill(.primary.opacity(0.7)).frame(width: 14, height: 0.75)
                Text("Tone as written")
                Rectangle().stroke(.primary, lineWidth: 0.75).frame(width: 4, height: 4).padding(.leading, 6)
                Text("As its words read. Higher is more hopeful.")
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
    }
}
