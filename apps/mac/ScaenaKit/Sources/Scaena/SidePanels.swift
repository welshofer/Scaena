import ScaenaKit
import SwiftUI

/// The inspector's tabs (PLAN 3.18), as a presentation app's: Format, the object selected or the
/// slide; Animate, its builds or the slide's transition; Document, the theme, the data, the media,
/// and the history (PLAN 3.15).
enum InspectorTab: String, CaseIterable, Identifiable {
    case format, animate, document

    var id: String { rawValue }

    var title: String {
        switch self {
        case .format: "Format"
        case .animate: "Animate"
        case .document: "Document"
        }
    }

    var symbol: String {
        switch self {
        case .format: "paintbrush"
        case .animate: "rhombus"
        case .document: "doc.text"
        }
    }

    var help: String {
        switch self {
        case .format: "Format: the object selected, or the slide"
        case .animate: "Animate: the object's builds, or the slide's transition"
        case .document: "Document: the theme, the data, the media, and the history"
        }
    }
}

/// What the Document tab shows (PLAN 3.15, 3.18): the theme, a data source as a table, the
/// bundle's pictures, fonts, and data files, and the versions its history keeps.
enum DocumentPanel: String, CaseIterable, Identifiable {
    case theme, data, media, history

    var id: String { rawValue }

    var title: String {
        switch self {
        case .theme: "Theme"
        case .data: "Data"
        case .media: "Media"
        case .history: "History"
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

/// The inspector's column (PLAN 3.18): the tab chosen. On the Mac the toolbar's Format, Animate,
/// and Document choose it, as a presentation app's do; on the iPad, whose toolbar folds, the
/// column's own control does too.
struct InspectorColumn<Formatting: View, Animating: View>: View {
    let editor: DeckEditor
    @Binding var tab: InspectorTab
    @Binding var document: DocumentPanel
    let edits: PanelEdits
    @ViewBuilder let format: () -> Formatting
    @ViewBuilder let animate: () -> Animating

    var body: some View {
        VStack(spacing: 0) {
            #if !os(macOS)
            Picker("Inspector", selection: $tab) {
                ForEach(InspectorTab.allCases) { tab in
                    Text(tab.title).tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(8)
            Divider()
            #endif
            switch tab {
            case .format: format()
            case .animate: animate()
            case .document: documentTab
            }
        }
    }

    /// The deck as a whole: its theme, its data, its media, and its history, one at a time.
    private var documentTab: some View {
        VStack(spacing: 0) {
            Picker("Document", selection: $document) {
                ForEach(DocumentPanel.allCases) { panel in
                    Text(panel.title).tag(panel)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(8)
            Divider()
            switch document {
            case .theme: ThemePanel(editor: editor, edits: edits)
            case .data: DataPanel(editor: editor, edits: edits)
            case .media: FilesPanel(editor: editor, edits: edits)
            case .history: VersionsPanel(editor: editor, edits: edits)
            }
        }
    }
}
