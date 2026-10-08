import AppKit
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

/// The deck's exports (PLAN 3.8): the PDF `scaena export` writes, a page for each slide at its
/// last state, and the state shown as a PNG, as the editor's Export… makes them. Each is written
/// when it is asked for, into a folder of its own, then shared through the Share sheet, shown in
/// Quick Look, or saved where the user says.
enum Exporting {
    /// What is exported.
    enum Kind {
        /// The deck as a PDF.
        case pdf
        /// A state at rest as a PNG, this many pixels wide.
        case png(state: String, width: Int)

        var type: UTType {
            switch self {
            case .pdf: .pdf
            case .png: .png
            }
        }
    }

    /// The export's bytes.
    static func bytes(_ kind: Kind, of editor: DeckEditor) throws -> Data {
        switch kind {
        case .pdf:
            return try editor.session.pdf()
        case .png(let state, let width):
            return try editor.session.png(state, width: width)
        }
    }

    /// The export written into a folder of its own, named for the deck: what the Share sheet and
    /// Quick Look are given.
    static func file(_ kind: Kind, of editor: DeckEditor) throws -> URL {
        let folder = FileManager.default.temporaryDirectory.appending(path: "Scaena-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appending(path: name(kind, of: editor))
        try bytes(kind, of: editor).write(to: url)
        return url
    }

    /// The export's file name: the deck's title, and for a PNG its state.
    static func name(_ kind: Kind, of editor: DeckEditor) -> String {
        let read = try? editor.session.tool("deck_read", [:])
        let title = read?.result["deck"]?["meta"]?["title"]?.string ?? "Deck"
        let safe = title.components(separatedBy: CharacterSet(charactersIn: "/:\\?%*|\"<>")).joined(separator: "-")
        switch kind {
        case .pdf: return "\(safe).pdf"
        case .png(let state, _): return "\(safe) – \(state).png"
        }
    }

    /// `url` offered through the Share sheet, from the window's top right.
    @MainActor static func share(_ url: URL) {
        guard let view = NSApp.keyWindow?.contentView else { return }
        let picker = NSSharingServicePicker(items: [url])
        let corner = NSRect(x: view.bounds.maxX - 80, y: view.bounds.maxY - 8, width: 1, height: 1)
        picker.show(relativeTo: corner, of: view, preferredEdge: .minY)
    }
}

/// An export's bytes, as a save panel writes them.
struct ExportedFile: FileDocument {
    static var readableContentTypes: [UTType] { [.pdf, .png] }
    let data: Data

    init(data: Data) {
        self.data = data
    }

    init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents else { throw CocoaError(.fileReadCorruptFile) }
        self.data = data
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: data)
    }
}
