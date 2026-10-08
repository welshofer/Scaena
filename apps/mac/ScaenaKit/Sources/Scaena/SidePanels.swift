import ScaenaKit
import SwiftUI

/// The panels beside the canvas (PLAN 3.15), as the browser's tabs at the right: the inspector, the
/// theme, the data, the bundle's files, and the versions its history keeps.
enum SidePanel: String, CaseIterable, Identifiable {
    case inspector, theme, data, files, versions

    var id: String { rawValue }

    var title: String {
        switch self {
        case .inspector: "Node"
        case .theme: "Theme"
        case .data: "Data"
        case .files: "Files"
        case .versions: "Versions"
        }
    }
}

/// What the panels do to the deck (PLAN 3.15): each edit one step of the window's undo; one that
/// throws, refused or failed, makes nothing, and the window says why.
struct PanelEdits {
    /// An edit made on the session beside the source (the theme edited or another taken, a version
    /// restored, rows written inline): the deck read again, its undo writing back the files the
    /// edit gives.
    let beside: (_ edit: () throws -> [Rewritten]) -> Void
    /// A data file written, or a file taken out: the deck read again, its undo the session's.
    let data: (_ edit: () throws -> Void) -> Void
}

/// An edit the deck refused, and why: thrown, nothing made, the window says why.
struct Refused: Error, CustomStringConvertible {
    let description: String
}

/// The panel chosen, under the buttons that choose it.
struct SidePanels<Inspecting: View>: View {
    let editor: DeckEditor
    @Binding var panel: SidePanel
    let edits: PanelEdits
    /// The inspector, the first panel.
    @ViewBuilder let inspector: () -> Inspecting

    var body: some View {
        VStack(spacing: 0) {
            Picker("Panel", selection: $panel) {
                ForEach(SidePanel.allCases) { panel in
                    Text(panel.title).tag(panel)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(8)
            Divider()
            switch panel {
            case .inspector: inspector()
            case .theme: ThemePanel(editor: editor, edits: edits)
            case .data: DataPanel(editor: editor, edits: edits)
            case .files: FilesPanel(editor: editor, edits: edits)
            case .versions: VersionsPanel(editor: editor, edits: edits)
            }
        }
    }
}
