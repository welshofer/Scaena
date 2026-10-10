import ScaenaKit
import SwiftUI

/// What compiling and lint found (PLAN 3.4, SPEC §7.5), errors first: each said, with the slide
/// it is about and the size it shows in, as a person reads them (PLAN 3.18). A click shows its
/// slide with its object selected; Fix applies a fix lint has checked, one step to undo; and where
/// the Mac has the on-device model, Explain says what the finding means for this deck (PLAN 3.6).
struct FindingsPanel: View {
    let editor: DeckEditor
    let go: (Finding) -> Void
    /// Explain a finding on this Mac; none where the on-device model is absent.
    let explain: ((Finding) -> Void)?
    let fix: (Finding) -> Void

    /// Advice for a deck placed by its theme's grid, which a person placing freely does not take
    /// (ADR-0024): a node placed by a `rect` (W301), or by cells in a deck with formats (W302). The
    /// CLI, the MCP server, and the browser still say it.
    static let freely: Set<String> = ["W301", "W302"]

    var body: some View {
        let found = ordered(editor.findings.filter { !Self.freely.contains($0.code) })
        List {
            ForEach(found.indices, id: \.self) { i in
                FindingRow(finding: found[i], about: about(found[i]), explain: explain, fix: fix)
                    .contentShape(Rectangle())
                    .onTapGesture { go(found[i]) }
            }
        }
        .listStyle(.inset)
        .overlay {
            if found.isEmpty {
                Label(
                    editor.whole ? "No issues on any slide" : "Checking every slide…",
                    systemImage: editor.whole ? "checkmark.circle" : "hourglass")
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// Where a finding is, as a person finds it: its slide, and the size it shows in where the
    /// deck has others; the source where it is about no slide.
    private func about(_ finding: Finding) -> String {
        var parts: [String] = []
        if let state = finding.state { parts.append(Words.slide(state, in: editor.slots)) }
        if let format = finding.format { parts.append("in \(format)") }
        return parts.isEmpty ? "The deck" : parts.joined(separator: " · ")
    }

    /// Errors, then warnings, then the rest; in the format shown first; else as lint lists them.
    private func ordered(_ findings: [Finding]) -> [Finding] {
        findings.enumerated()
            .sorted { a, b in
                if a.element.severity != b.element.severity { return a.element.severity > b.element.severity }
                if a.element.shown != b.element.shown { return a.element.shown }
                return a.offset < b.offset
            }
            .map(\.element)
    }
}

private struct FindingRow: View {
    let finding: Finding
    /// Where it is, in a person's words.
    let about: String
    let explain: ((Finding) -> Void)?
    let fix: (Finding) -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: symbol).foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(finding.message).lineLimit(3)
                Text(about).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            if let explain {
                Button {
                    explain(finding)
                } label: {
                    Image(systemName: "sparkles")
                }
                .buttonStyle(.borderless)
                .help("What this means for the deck, answered on this Mac")
            }
            if finding.fixable, finding.fix != nil {
                Button("Fix") { fix(finding) }
                    .help(finding.hint ?? "Apply the fix lint has checked")
            }
        }
        .padding(.vertical, 2)
    }

    private var symbol: String {
        switch finding.severity {
        case .error: "xmark.octagon.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .info: "info.circle"
        }
    }

    private var tint: Color {
        switch finding.severity {
        case .error: .red
        case .warning: .orange
        case .info: .secondary
        }
    }
}
