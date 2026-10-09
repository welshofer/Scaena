import ScaenaKit
import SwiftUI

/// What lint finds that a person can see on a slide and put right there (PLAN 3.20), as a sentence
/// they would say: text that does not fit or cannot be read, objects that overlap, chart text too
/// small, a picture with no description, a figure its data no longer gives. The rest (the deck's
/// structure, its story, its motion, its placement) stays in the View menu's Issues, and with the
/// CLI, the MCP server, and the browser.
enum Issues {
    /// A finding as the canvas says it, where it is one the canvas marks.
    static func said(_ finding: Finding) -> String? {
        switch finding.code {
        case "E100": "The text doesn't fit its box."
        case "E101": "It overlaps another object."
        case "E110", "E111": "It's hard to read against what's behind it."
        case "E120": "Some of its characters aren't in its font."
        case "W202": "It runs to more lines than its style allows."
        case "W203": "It doesn't fit, even at its smallest size."
        case "W231": "Its font has no italic, so it's set upright."
        case "W310": "Some of the chart's labels overlap."
        case "W311": "A background effect is behind this chart."
        case "W312": "Some of the chart's text is too small to read."
        case "W313": "The chart is too small to read."
        case "W410": "The picture has no description for people who can't see it."
        case "W427": "A figure here no longer matches its data."
        default: nil
        }
    }

    /// What its fix does, as a button says it.
    static func fixing(_ finding: Finding) -> String {
        switch finding.code {
        case "E110", "E111": "Use a Color That Reads"
        case "E100", "W202": "Shrink to Fit"
        case "W427": "Update the Figure"
        default: "Fix"
        }
    }

    /// The findings the canvas marks on each object in `state`, in the format shown, errors first.
    static func marked(_ findings: [Finding], in state: String) -> [String: [Finding]] {
        var marked: [String: [Finding]] = [:]
        for finding in findings where finding.state == state && finding.shown && said(finding) != nil {
            guard let node = finding.node else { continue }
            marked[node, default: []].append(finding)
        }
        return marked.mapValues { found in found.sorted { $0.severity > $1.severity } }
    }
}

/// A mark on an object the canvas found something wrong with (PLAN 3.20): red for what is wrong,
/// orange for what reads badly; a click says what, in a sentence, with the fix where lint has one.
struct IssueBadge: View {
    let issues: [Finding]
    /// Apply a finding's fix: one step to undo.
    let fix: (Finding) -> Void
    @State private var open = false

    private var error: Bool { issues.contains { $0.severity == .error } }

    var body: some View {
        Button {
            open.toggle()
        } label: {
            Image(systemName: error ? "exclamationmark.circle.fill" : "exclamationmark.triangle.fill")
                .symbolRenderingMode(.palette)
                .foregroundStyle(.white, error ? Color.red : Color.orange)
                .font(.system(size: 15, weight: .semibold))
                .padding(3)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(issues.compactMap(Issues.said).joined(separator: " "))
        .accessibilityLabel(error ? "Problem" : "Warning")
        .accessibilityValue(issues.compactMap(Issues.said).joined(separator: " "))
        .popover(isPresented: $open) {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(issues.indices, id: \.self) { i in
                    let issue = issues[i]
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Image(systemName: issue.severity == .error ? "exclamationmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(issue.severity == .error ? Color.red : Color.orange)
                        VStack(alignment: .leading, spacing: 6) {
                            Text(Issues.said(issue) ?? "")
                                .fixedSize(horizontal: false, vertical: true)
                            if issue.fixable, issue.fix != nil {
                                Button(Issues.fixing(issue)) {
                                    open = false
                                    fix(issue)
                                }
                                .controlSize(.small)
                            }
                        }
                    }
                }
            }
            .padding(12)
            .frame(width: 280, alignment: .leading)
        }
    }
}
