import ScaenaKit
import SwiftUI

/// The versions the bundle's history keeps (PLAN 3.15, as the browser's Versions tab, PLAN 2.60),
/// each change by what it did, who made it, and when, in the engine's words as the browser says
/// them (PLAN 2.99). One chosen is drawn read only, a slide of it at a time, numbered as its own
/// slide list numbers them, and
/// compared with the deck now; Restore makes it the deck again with its data files and its theme,
/// one step to undo, which writes the files back, and Restore This Slide puts back the slide shown
/// alone, with the data it reads, the rest of the deck as it is, as the browser's does (dogfood
/// 16). Where the bundle keeps no history, Keep a History
/// begins one with the next save (PLAN 2.87). A version is replayed, compared, and drawn on a
/// snapshot's session away from the window, and the list is read again as a save adds to it, not
/// on every edit (lift-up 20261110 PERF-3).
struct VersionsPanel: View {
    let editor: DeckEditor
    let edits: PanelEdits
    @State private var versions: [Version] = []
    @State private var keeps = false
    @State private var chosen: String?
    @State private var states: [String] = []
    /// Each of the version's states as its slide list numbers it: Slide 3, step 2.
    @State private var slides: [String] = []
    @State private var state: String?
    @State private var drawing: Image?
    @State private var compared: Compared?
    @State private var problem: String?
    /// What the last restore of a slide put back, as the browser's status says it.
    @State private var putBack: String?
    /// Each read of the versions begun, counted, so an answer to one before the last is let go.
    @State private var reading = 0
    /// The version chosen, shown on a snapshot's session of its own.
    @State private var version: VersionShown?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if versions.isEmpty {
                none
            } else {
                List(Array(versions.reversed()), selection: $chosen) { version in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(version.said).lineLimit(2)
                        Text("\(version.by) · \(Self.when(version))").font(.caption).foregroundStyle(.secondary)
                    }
                    .tag(version.id)
                }
                .frame(minHeight: 120)
                if chosen != nil { shown }
            }
        }
        .onAppear { read() }
        .onReceive(NotificationCenter.default.publisher(for: ScaenaDocument.saved)) { saved in
            if (saved.object as AnyObject?) === editor.session { read() }
        }
        .task(id: chosen) { await view() }
        .task(id: "\(chosen ?? "")\u{1f}\(state ?? "")") { await draw() }
    }

    /// Where there are no versions: why, and how a history begins.
    @ViewBuilder private var none: some View {
        if keeps {
            Text("The bundle's history begins with the next save: each save after records the edits since.")
                .foregroundStyle(.secondary)
                .padding(8)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                Text("The bundle keeps no history.").foregroundStyle(.secondary)
                Button("Keep a History") { keep() }
                    .help("Begin a history with the next save, as `scaena save --history` does")
            }
            .padding(8)
        }
    }

    /// The version chosen: a state of it drawn, what changed since, and Restore.
    private var shown: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !states.isEmpty {
                Picker("Slide", selection: $state) {
                    ForEach(Array(states.enumerated()), id: \.offset) { i, id in
                        Text(i < slides.count ? slides[i] : "Slide \(i + 1)").tag(Optional(id))
                    }
                }
            }
            if let drawing {
                drawing.resizable().aspectRatio(contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(.separator))
            }
            if let compared { Text(Self.since(compared)).font(.caption).foregroundStyle(.secondary) }
            if let problem { Text(problem).font(.caption).foregroundStyle(.red) }
            if let putBack { Text(putBack).font(.caption).foregroundStyle(.secondary) }
            HStack {
                Button("Restore This Version") { restore() }
                    .help("Make this version the deck again, with its data files and its theme: one step to undo")
                if let picked {
                    // The slide it puts back said before it is pressed, as the slide picker names it.
                    Button("Restore This Slide") { restore(slide: picked.state) }
                        .help("Put back \(picked.said) as this version drew it, with the data it reads, the rest of the deck as it is: one step to undo")
                        .accessibilityHint("Puts back \(picked.said) as this version drew it")
                        .accessibilityIdentifier("restore-slide")
                }
            }
        }
        .padding(8)
    }

    /// The state the slide picker shows, and its slide as the picker names it.
    private var picked: (state: String, said: String)? {
        guard let state, let i = states.firstIndex(of: state) else { return nil }
        return (state, i < slides.count ? slides[i] : "Slide \(i + 1)")
    }

    /// The versions read again, away from the window (editor dogfood 7): a change its message says
    /// nothing of is said by what it changed, which checks the newest versions out. The last read
    /// begun is the one kept.
    private func read() {
        let session = editor.session
        keeps = (try? session.keepsHistory()) ?? false
        reading += 1
        let asked = reading
        guard keeps, let snapshot = try? session.snapshot() else {
            versions = []
            return
        }
        Task {
            let listed = (try? await VersionShown.listed(from: snapshot)) ?? []
            guard asked == reading else { return }
            versions = listed
            if let chosen, !versions.contains(where: { $0.id == chosen }) { self.chosen = nil }
        }
    }

    /// The version chosen shown read only, on a snapshot's session of its own away from the window,
    /// and compared with the deck now.
    private func view() async {
        drawing = nil
        compared = nil
        problem = nil
        putBack = nil
        version = nil
        guard let chosen else {
            states = []
            slides = []
            return
        }
        do {
            let shown = try await VersionShown.opened(chosen, from: try editor.session.snapshot())
            guard self.chosen == chosen else { return }
            version = shown
            states = shown.states
            slides = shown.slides
            compared = shown.compared
            if state == nil || !states.contains(state ?? "") { state = states.first }
            await draw()
        } catch is CancellationError {
        } catch {
            problem = Why.said(error)
        }
    }

    private func draw() async {
        guard let version, let state, version.states.contains(state) else { return }
        do {
            let png = try await version.png(state, width: 640)
            guard self.version === version, self.state == state else { return }
            drawing = Image(png: png)
        } catch is CancellationError {
        } catch {
            drawing = nil
            problem = Why.said(error)
        }
    }

    private func keep() {
        do {
            try editor.session.keepHistory()
            keeps = true
        } catch {
            problem = Why.said(error)
        }
    }

    /// The version chosen made the deck again, or with `slide`, a state of it, the slide that holds
    /// it put back alone: one step to undo, and what it put back said, as the browser says it.
    private func restore(slide: String? = nil) {
        guard let chosen else { return }
        let n = versions.first { $0.id == chosen }?.n
        let of = n.map { "version \($0)" } ?? "this version"
        let before = editor.source
        var put: Restored?
        let made = edits.beside(slide == nil ? "Restore Version" : "Restore Slide") {
            let restored = try editor.session.restoreVersion(chosen, slide: slide)
            guard restored.applied else {
                throw Refused(
                    slide == nil ? "not restored" : "The slide of \(of) not put back", restored.refusals, by: editor,
                    otherwise: "the deck would not open with the files it has now")
            }
            put = restored
            return restored.files
        }
        guard made, let restored = put else { return putBack = nil }
        // What it shows as it is now, for another slide's sake, said (dogfood 16).
        let kept = restored.kept.map { ". \($0)" }.joined()
        // Nothing to put back: the deck and its files are already as the version had them, a slide
        // the deck already draws as the version did, so no step to undo was made, which ⌘Z would
        // take back to the same deck (editor dogfood 7).
        if editor.source == before, restored.files.isEmpty {
            let asNow = slide != nil ? "\(restored.slide ?? "The slide") is as \(of) drew it" : "The deck is as \(of) drew it"
            putBack = "Nothing put back: \(asNow)\(kept)"
            return
        }
        guard slide != nil else { return putBack = nil }
        let files = restored.files.map { $0.path.split(separator: "/").last.map(String.init) ?? $0.path }
        let with = files.isEmpty ? "" : ", with \(files.joined(separator: ", "))"
        putBack = "\(restored.slide ?? "The slide") put back from \(of)\(with)\(kept) · ⌘Z undoes it"
    }

    /// When a version was made, as the list says it.
    private static func when(_ version: Version) -> String {
        guard let at = version.at, let date = ISO8601DateFormatter().date(from: at) else {
            return "\(version.ops) changes"
        }
        return "\(date.formatted(date: .abbreviated, time: .shortened)) · \(version.ops) changes"
    }

    /// What changed from a version to the deck now, as a sentence says it.
    private static func since(_ compared: Compared) -> String {
        if compared.states.isEmpty && compared.deck.isEmpty && compared.files.isEmpty && compared.story.isEmpty {
            return "The deck now is as this version was."
        }
        var said: [String] = []
        let n = compared.states.count
        if n > 0 { said.append("\(n) slide\(n == 1 ? "" : "s") changed since") }
        // A beat's fields, named by the slides they are read on (editor dogfood 7), as
        // `deck_history` names them: "Presenter Notes on Slide 4".
        for entry in compared.story { said.append("\(entry.label) on \(Self.and(entry.slides))") }
        if !compared.deck.isEmpty { said.append("the deck's settings changed") }
        if !compared.files.isEmpty {
            said.append("\(compared.files.count) file\(compared.files.count == 1 ? "" : "s") changed")
        }
        return said.joined(separator: "; ").prefix(1).uppercased() + said.joined(separator: "; ").dropFirst()
    }

    /// `items` in a sentence: "a", "a and b", "a, b, and c".
    private static func and(_ items: [String]) -> String {
        switch items.count {
        case 0: return ""
        case 1: return items[0]
        case 2: return "\(items[0]) and \(items[1])"
        default: return "\(items.dropLast().joined(separator: ", ")), and \(items.last!)"
        }
    }
}
