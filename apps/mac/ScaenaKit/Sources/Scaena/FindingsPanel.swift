import ScaenaKit
import SwiftUI

/// What compiling and lint found (PLAN 3.4, SPEC §7.5), errors first: each with its code, the
/// state and node it is about, and where the source sets it. A click shows its state with its
/// node selected; Fix applies a fix lint has checked, one step to undo; and where the Mac has the
/// on-device model, Explain says what the finding means for this deck (PLAN 3.6).
struct FindingsPanel: View {
    let editor: DeckEditor
    let go: (Finding) -> Void
    /// Explain a finding on this Mac; none where the on-device model is absent.
    let explain: ((Finding) -> Void)?
    let fix: (Finding) -> Void

    var body: some View {
        let found = ordered(editor.findings)
        List {
            if found.isEmpty {
                Text(editor.whole ? "Lint finds nothing." : "Linting every state once edits stop…")
                    .foregroundStyle(.secondary)
            }
            ForEach(found.indices, id: \.self) { i in
                FindingRow(finding: found[i], explain: explain, fix: fix)
                    .contentShape(Rectangle())
                    .onTapGesture { go(found[i]) }
            }
        }
        .listStyle(.inset)
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

    /// Its code, the state and node it is about, its format, and its line in the source.
    private var about: String {
        var parts = [finding.code]
        if let state = finding.state { parts.append(state) }
        if let node = finding.node { parts.append(node) }
        if let format = finding.format { parts.append("in \(format)") }
        if let at = finding.at { parts.append("line \(at.line)") }
        return parts.joined(separator: " · ")
    }
}
