import ScaenaKit
import SwiftUI

/// What compiling and lint found (PLAN 3.4, SPEC §7.5), errors first: each with its code, the
/// state and node it is about, and where the source sets it. A click shows its state with its
/// node selected; Fix applies a fix lint has checked, one step to undo.
struct FindingsPanel: View {
    let editor: DeckEditor
    let go: (Finding) -> Void
    let fix: (Finding) -> Void

    var body: some View {
        let found = ordered(editor.findings)
        List {
            if found.isEmpty {
                Text(editor.whole ? "Lint finds nothing." : "Linting every state once edits stop…")
                    .foregroundStyle(.secondary)
            }
            ForEach(found.indices, id: \.self) { i in
                FindingRow(finding: found[i], fix: fix)
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
    let fix: (Finding) -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: symbol).foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(finding.message).lineLimit(3)
                Text(about).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
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
