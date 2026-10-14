#if os(macOS)
import AppKit
#endif
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

/// What the Document tab shows (PLAN 3.15, 3.18, G.9): the theme, a data source as a table, the
/// bundle's pictures, fonts, and data files, the versions its history keeps, and the deck's story.
enum DocumentPanel: String, CaseIterable, Identifiable {
    case theme, data, media, history, story

    var id: String { rawValue }

    var title: String {
        switch self {
        case .theme: "Theme"
        case .data: "Data"
        case .media: "Media"
        case .history: "History"
        case .story: "Story"
        }
    }
}

/// What the panels do to the deck (PLAN 3.15): each edit one step of the window's undo, named as
/// the Edit menu's Undo says it (PLAN 3.28); one that throws, refused or failed, makes nothing,
/// and the window says why.
struct PanelEdits {
    /// An edit made on the session beside the source (the theme edited or another taken, a version
    /// restored, rows written inline): the deck read again, its undo writing back the files the
    /// edit gives. Whether it was made.
    let besideMade: (_ name: String, _ edit: () throws -> [Rewritten]) -> Bool
    /// A data file written, or a file taken out: the deck read again, its undo the session's.
    /// Whether it was made.
    let dataMade: (_ name: String, _ edit: () throws -> Void) -> Bool
    /// A patch made by the user, as a gesture's is (the story's fix, PLAN G.9). Whether it was.
    let patchMade: (_ name: String, _ ops: [JSONValue]) -> Bool
    /// Show the state, as a click on it in the slides does.
    let go: (_ state: String) -> Void
    /// Put a question to the assistant: its panel shown, the question in its box to send.
    let ask: (_ question: String) -> Void
    /// Say what an edit did, as the window says it for a moment.
    let say: (_ words: String) -> Void

    /// `edit` made beside the source, one step to undo named `name`: whether it was.
    @discardableResult
    func beside(_ name: String, _ edit: () throws -> [Rewritten]) -> Bool { besideMade(name, edit) }

    /// `edit` of a file, one step to undo named `name`: whether it was made.
    @discardableResult
    func data(_ name: String, _ edit: () throws -> Void) -> Bool { dataMade(name, edit) }

    /// `ops` made by the user, one step to undo named `name`: whether it was made.
    @discardableResult
    func patch(_ name: String, _ ops: [JSONValue]) -> Bool { patchMade(name, ops) }
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

    /// The deck as a whole: its theme, its data, its media, its history, and its story, one at a
    /// time.
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
            case .story: StoryPanel(editor: editor, edits: edits)
            }
        }
    }
}

#if os(macOS)
/// The inspector's tabs kept in the toolbar as the window narrows (PLAN 3.18, 3.27), as a
/// presentation app keeps Format, Animate, and Document at its toolbar's end: the other items go
/// into the toolbar's overflow first, so the toolbar always shows which tab the inspector is open
/// at. SwiftUI says nothing of an item's priority, so the window's own toolbar items are given it,
/// found by their ids or their titles, as the toolbar is made and as the window's size changes. It
/// takes no press.
struct ToolbarKeeps: NSViewRepresentable {
    /// The ids of the items kept, and their titles.
    let ids: [String]
    let titles: [String]

    func makeNSView(context: Context) -> KeepsView {
        let view = KeepsView()
        updateNSView(view, context: context)
        return view
    }

    func updateNSView(_ view: KeepsView, context: Context) {
        view.ids = Set(ids)
        view.titles = Set(titles)
        view.keep()
    }

    final class KeepsView: NSView {
        var ids: Set<String> = []
        var titles: Set<String> = []

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard window != nil else { return }
            keep()
            // The toolbar's items are made once the window shows: kept again then.
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(500))
                self?.keep()
            }
        }

        override func setFrameSize(_ newSize: NSSize) {
            super.setFrameSize(newSize)
            keep()
        }

        /// Each item kept made high in priority, so that the others overflow first.
        func keep() {
            for item in window?.toolbar?.items ?? [] where item.visibilityPriority != .high && keeps(item) {
                item.visibilityPriority = .high
            }
        }

        private func keeps(_ item: NSToolbarItem) -> Bool {
            let id = item.itemIdentifier.rawValue
            return ids.contains(id) || ids.contains { id.hasSuffix(".\($0)") } || titles.contains(item.label)
        }
    }
}
#endif
