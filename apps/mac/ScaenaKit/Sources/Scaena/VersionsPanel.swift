import ScaenaKit
import SwiftUI

/// The versions the bundle's history keeps (PLAN 3.15, as the browser's Versions tab, PLAN 2.60),
/// each change by its author and time. One chosen is drawn read only, a state of it at a time, and
/// compared with the deck now; Restore makes it the deck again with its data files and its theme,
/// one step to undo, which writes the files back. Where the bundle keeps no history, Keep a History
/// begins one with the next save (PLAN 2.87).
struct VersionsPanel: View {
    let editor: DeckEditor
    let edits: PanelEdits
    @State private var versions: [Version] = []
    @State private var keeps = false
    @State private var chosen: String?
    @State private var states: [String] = []
    @State private var state: String?
    @State private var drawing: Image?
    @State private var compared: Compared?
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if versions.isEmpty {
                none
            } else {
                List(Array(versions.reversed()), selection: $chosen) { version in
                    VStack(alignment: .leading, spacing: 2) {
                        Text("#\(version.n) · \(version.author ?? "someone")")
                        Text(Self.when(version)).font(.caption).foregroundStyle(.secondary)
                        if let message = version.message {
                            Text(message).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                    .tag(version.id)
                }
                .frame(minHeight: 120)
                if chosen != nil { shown }
            }
        }
        .task(id: editor.revision) { read() }
        .task(id: chosen) { view() }
        .task(id: "\(chosen ?? "")\u{1f}\(state ?? "")") { draw() }
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
                Picker("State", selection: $state) {
                    ForEach(states, id: \.self) { Text($0).tag(Optional($0)) }
                }
            }
            if let drawing {
                drawing.resizable().aspectRatio(contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(.separator))
            }
            if let compared { Text(Self.since(compared)).font(.caption).foregroundStyle(.secondary) }
            if let problem { Text(problem).font(.caption).foregroundStyle(.red) }
            Button("Restore This Version") { restore() }
                .help("Make this version the deck again, with its data files and its theme: one step to undo")
        }
        .padding(8)
    }

    private func read() {
        let session = editor.session
        keeps = (try? session.keepsHistory()) ?? false
        versions = (try? session.versions()) ?? []
        if let chosen, !versions.contains(where: { $0.id == chosen }) { self.chosen = nil }
    }

    /// The version chosen shown read only, in a session of its own, and compared with the deck now.
    private func view() {
        drawing = nil
        compared = nil
        problem = nil
        guard let chosen else {
            states = []
            return
        }
        do {
            states = try editor.session.viewVersion(chosen)
            if state == nil || !states.contains(state ?? "") { state = states.first }
            compared = try editor.session.compareVersions(from: chosen)
        } catch {
            problem = "\(error)"
        }
    }

    private func draw() {
        guard chosen != nil, let state, states.contains(state) else { return }
        do {
            drawing = Image(png: try editor.session.versionPNG(state, width: 640))
        } catch {
            drawing = nil
            problem = "\(error)"
        }
    }

    private func keep() {
        do {
            try editor.session.keepHistory()
            keeps = true
        } catch {
            problem = "\(error)"
        }
    }

    private func restore() {
        guard let chosen else { return }
        edits.beside("Restore Version") {
            let restored = try editor.session.restoreVersion(chosen)
            guard restored.applied else {
                let why = restored.why.first ?? "it would not validate in the bundle as it is"
                throw Refused(description: "not restored: \(why)")
            }
            return restored.files
        }
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
        if compared.states.isEmpty && compared.deck.isEmpty && compared.files.isEmpty {
            return "The deck now is as this version was."
        }
        var said: [String] = []
        let n = compared.states.count
        if n > 0 { said.append("\(n) slide\(n == 1 ? "" : "s") changed since") }
        if !compared.deck.isEmpty { said.append("the deck's settings changed") }
        if !compared.files.isEmpty {
            said.append("\(compared.files.count) file\(compared.files.count == 1 ? "" : "s") changed")
        }
        return said.joined(separator: "; ").prefix(1).uppercased() + said.joined(separator: "; ").dropFirst()
    }
}
