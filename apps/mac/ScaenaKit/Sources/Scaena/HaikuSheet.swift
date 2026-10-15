#if os(macOS)
import AppKit
#endif
import ScaenaKit
import SwiftUI

/// Haiku… (PLAN O.4g, ADR-0030 decisions 26–29), as the browser's dialog asks: what it is about (a
/// topic, this slide, or the whole talk), where it goes (a new slide after this one, or this one;
/// not asked where a verse is selected or the slide waits for one), and who wrote it or what it is
/// on, never filled in for the person. Write asks the model on this device, else the assistant's
/// with the user's key (`Writer`), once more where the poem is off 5-7-5 (`Haiku.written`), and
/// offers the poem to edit, each line's count beside it, a line off its count marked. Use It hands
/// the patch to `put`: one step to undo. Which model wrote it goes in the slide's notes, never on
/// the slide.
struct HaikuSheet: View {
    let session: ScaenaSession
    let target: HaikuTarget
    /// The poem taken, placed.
    let put: (HaikuPlaced) -> Void

    @Environment(\.dismiss) private var dismiss
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #else
    /// The keys, on the iPad in a sheet: it has no Settings window (PLAN 4.2).
    @State private var keying = false
    #endif
    @State private var about: HaikuAbout = .topic
    @State private var topic = ""
    @State private var place: HaikuPlace = .new
    @State private var attribution = ""
    @State private var lines: [String] = []
    @State private var checked: VerseChecked?
    @State private var note: String?
    @State private var status = ""
    @State private var asked: HaikuAsk?
    @State private var writing: Task<Void, Never>?
    /// Bumped as the keys sheet closes, or, on the Mac, as a window comes back from Settings, so
    /// that the keys are read again.
    @State private var keysChanged = 0

    /// Whether there is a model to ask.
    private var available: Bool {
        _ = keysChanged
        return Writer.available
    }

    /// Who writes it, as its slide's notes will say it.
    private var writer: HaikuWriter? {
        if OnDevice.available { return HaikuWriter(model: "the model on this \(Words.device)") }
        return Writer.byKey().map { HaikuWriter(model: $0.model, provider: $0.provider.name) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Haiku")
                .font(.headline)
            Picker("About", selection: $about) {
                Text("A Topic").tag(HaikuAbout.topic)
                Text("This Slide").tag(HaikuAbout.slide)
                Text("The Whole Talk").tag(HaikuAbout.talk)
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("haiku-about")
            if about == .topic {
                TextField("The first frost on the trail", text: $topic)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel("What the haiku is about")
                    .accessibilityIdentifier("haiku-topic")
            }
            if target.asksWhere {
                Picker("Put it on", selection: $place) {
                    Text("A New Slide After This One").tag(HaikuPlace.new)
                    Text("This Slide").tag(HaikuPlace.slide)
                }
                .accessibilityIdentifier("haiku-where")
            }
            TextField("Who wrote it, or what it is on (optional)", text: $attribution)
                .textFieldStyle(.roundedBorder)
                .accessibilityLabel("Attribution")
                .accessibilityIdentifier("haiku-attribution")
            if !lines.isEmpty {
                ForEach(lines.indices, id: \.self) { i in
                    HStack {
                        TextField(["The first line", "The second line", "The third line"][min(i, 2)], text: $lines[i])
                            .textFieldStyle(.roundedBorder)
                            .accessibilityIdentifier("haiku-line-\(i)")
                            .accessibilityValue(counted(i))
                        Text(count(i))
                            .monospacedDigit()
                            .frame(minWidth: 48, alignment: .leading)
                            .foregroundStyle(off(i) ? Color.red : Color.secondary)
                            .accessibilityHidden(true)
                    }
                }
                if let note {
                    Text(note).font(.callout).foregroundStyle(.secondary)
                }
            }
            if !status.isEmpty {
                Text(status)
                    .font(.callout)
                    .accessibilityIdentifier("haiku-status")
            }
            Text(available ? Writer.whence : unavailable)
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack {
                if !available {
                    Button("Add a Key…", action: addKey)
                }
                Spacer()
                Button("Leave It") {
                    writing?.cancel()
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                if lines.isEmpty {
                    Button("Write", action: { write(nil) })
                        .keyboardShortcut(.defaultAction)
                        .disabled(writing != nil || !available || (about == .topic && trimmed(topic).isEmpty))
                } else {
                    Button("Write Another", action: { write(lines.map(trimmed).filter { !$0.isEmpty }) })
                        .disabled(writing != nil || !available)
                    Button("Use It", action: use)
                        .keyboardShortcut(.defaultAction)
                        .disabled(writing != nil || lines.allSatisfy { trimmed($0).isEmpty })
                }
            }
        }
        .padding(20)
        .frame(minWidth: 460)
        .onChange(of: lines) { _, now in checked = try? session.verseCounts(now) }
        // Put away while a poem is coming: it is dropped, and nothing is placed.
        .onDisappear { writing?.cancel() }
        #if os(macOS)
        // Back from Settings, where Add a Key… sent the person: the keys kept are read again.
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            keysChanged += 1
        }
        #else
        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
            NavigationStack {
                KeysSettings()
                    .toolbar { Button("Done") { keying = false } }
            }
        }
        #endif
    }

    private var unavailable: String {
        #if os(macOS)
        "No model to ask: this \(Words.device) has none of its own, and no key is kept. Add one in Settings."
        #else
        "No model to ask: this \(Words.device) has none of its own, and no key is kept. Add one with Add a Key."
        #endif
    }

    private func trimmed(_ s: String) -> String { s.trimmingCharacters(in: .whitespacesAndNewlines) }

    private func line(_ i: Int) -> VerseChecked.Line? {
        guard let checked, checked.lines.indices.contains(i) else { return nil }
        return checked.lines[i]
    }

    /// A line's count, as the sheet shows it beside the line.
    private func count(_ i: Int) -> String { line(i)?.count?.said ?? "?" }

    private func off(_ i: Int) -> Bool { line(i)?.off ?? false }

    /// A line's count, as VoiceOver says it.
    private func counted(_ i: Int) -> String {
        guard let l = line(i), let target = l.target else { return "counts \(count(i))" }
        return l.off ? "counts \(count(i)), not \(target)" : "counts \(count(i))"
    }

    private func addKey() {
        #if os(macOS)
        openSettings()
        #else
        keying = true
        #endif
    }

    /// Ask for a poem; `another`, the poem not to write again.
    private func write(_ another: [String]?) {
        let ask = HaikuAsk(
            state: target.state, about: about, topic: about == .topic ? trimmed(topic) : nil,
            another: another.flatMap { $0.isEmpty ? nil : $0 })
        status = OnDevice.available ? "Thinking on this \(Words.device)…" : "Asking…"
        writing = Task { @MainActor in
            defer { writing = nil }
            do {
                let (poem, said) = try await Haiku.written(ask, of: session) { instructions, prompt in
                    try await Writer.respond(instructions: instructions, prompt: prompt)
                }
                guard !Task.isCancelled else { return }
                asked = ask
                lines = poem.lines
                checked = poem.checked
                note = said
                status = ""
            } catch {
                // Why no poem came, in a person's words (PLAN 3.18).
                if !Task.isCancelled { status = "No haiku was written. \(Why.said(error))" }
            }
        }
    }

    /// The poem as it stands in the sheet, placed: one patch, one step to undo.
    private func use() {
        guard let asked, let writer else { return }
        do {
            let taken = try session.haikuTaken(
                asked, lines: lines.map(trimmed), to: target.asksWhere ? place : .slide, node: target.node,
                at: target.at, attribution: trimmed(attribution), by: writer, date: Haiku.today())
            guard case .offer(_, _, let placed?) = taken else {
                status = "Not placed: the haiku has nowhere to go."
                return
            }
            put(placed)
            dismiss()
        } catch {
            status = "Not placed. \(Why.said(error))"
        }
    }
}
