#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

/// The deck's exports (PLAN 3.8): the PDF `scaena export` writes, a page for each slide at its
/// last state, or with the presenter's notes under each, as `--notes` writes it; the state shown
/// as a PNG, and every slide as PNGs in one zip, each named by its
/// number (PLAN 2.122), as the editor's Export… makes them. Each is written
/// when it is asked for, into a folder of its own, then shared through the Share sheet, shown in
/// Quick Look, or saved where the user says.
enum Exporting {
    /// What is exported.
    enum Kind {
        /// The deck as a PDF.
        case pdf
        /// The deck as a PDF with the presenter's notes: each slide smaller, its notes under it.
        case notes
        /// A state at rest as a PNG, this many pixels wide.
        case png(state: String, width: Int)
        /// Every slide once its steps have played, as PNGs this many pixels wide in one zip.
        case slides(width: Int)

        var type: UTType {
            switch self {
            case .pdf, .notes: .pdf
            case .png: .png
            case .slides: .zip
            }
        }
    }

    /// What the engine makes for `kind`, read on the window's thread: the deck's title names the
    /// slides in their zip.
    static func export(_ kind: Kind, of editor: DeckEditor) -> Export {
        switch kind {
        case .pdf: .pdf
        case .notes: .notes
        case .png(let state, let width): .png(state: state, width: width)
        case .slides(let width): .slides(width: width, name: title(of: editor))
        }
    }

    /// What the window says while `kind` is made (lift-up PERF-3).
    static func making(_ kind: Kind) -> String {
        switch kind {
        case .pdf: "Making the PDF…"
        case .notes: "Making the PDF with the notes…"
        case .png: "Making the PNG…"
        case .slides: "Making a PNG of every slide…"
        }
    }

    /// An export's bytes written into a folder of its own as `name`: what the Share sheet and
    /// Quick Look are given, and which each takes away once it is done (lift-up REL-5).
    static func file(_ data: Data, named name: String) throws -> URL {
        try ExportFolders.write(data, named: name)
    }

    /// The export's file name: the deck's title, and for a PNG its slide and step as the window
    /// says them, "Slide 3, step 2", never the state's id (lift-up 10-25b ATTR-3).
    static func name(_ kind: Kind, of editor: DeckEditor) -> String {
        let safe = title(of: editor)
        switch kind {
        case .pdf: return "\(safe).pdf"
        case .notes: return "\(safe) - Notes.pdf"
        case .png(let state, _): return "\(safe) – \(Words.slide(state, in: editor.slots)).png"
        case .slides: return "\(safe) - Slides.zip"
        }
    }

    /// The deck's title, as a file's name may hold it.
    static func title(of editor: DeckEditor) -> String {
        let title = (try? editor.session.title()).flatMap { $0 } ?? "Deck"
        return title.components(separatedBy: CharacterSet(charactersIn: "/:\\?%*|\"<>")).joined(separator: "-")
    }

    /// `url` offered through the Share sheet, from the window's top right, its folder taken away
    /// once the share is done, failed, or dismissed (lift-up REL-5).
    @MainActor static func share(_ url: URL) {
        #if os(macOS)
        guard let view = NSApp.keyWindow?.contentView else {
            ExportFolders.remove(url)
            return
        }
        let picker = NSSharingServicePicker(items: [url])
        let end = SharingEnd(url: url)
        SharingEnd.live.insert(end)
        picker.delegate = end
        let corner = NSRect(x: view.bounds.maxX - 80, y: view.bounds.maxY - 8, width: 1, height: 1)
        picker.show(relativeTo: corner, of: view, preferredEdge: .minY)
        #else
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard var top = scene?.keyWindow?.rootViewController else {
            ExportFolders.remove(url)
            return
        }
        while let shown = top.presentedViewController { top = shown }
        let sheet = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        sheet.completionWithItemsHandler = { _, _, _, _ in ExportFolders.remove(url) }
        // On the iPad the sheet is a popover, which must point at something.
        sheet.popoverPresentationController?.sourceView = top.view
        sheet.popoverPresentationController?.sourceRect = CGRect(x: top.view.bounds.maxX - 80, y: 8, width: 1, height: 1)
        top.present(sheet, animated: true)
        #endif
    }
}

#if os(macOS)
/// What ends a share on the Mac: the picker dismissed with nothing chosen, or the service chosen
/// done or failed. Each takes the export's folder away (lift-up REL-5).
@MainActor
private final class SharingEnd: NSObject, NSSharingServicePickerDelegate, NSSharingServiceDelegate {
    /// Each share under way, kept until it ends, since the picker and the service hold their
    /// delegates weakly.
    static var live: Set<SharingEnd> = []
    let url: URL

    init(url: URL) {
        self.url = url
    }

    func sharingServicePicker(
        _ picker: NSSharingServicePicker, delegateFor sharingService: NSSharingService
    ) -> (any NSSharingServiceDelegate)? {
        self
    }

    func sharingServicePicker(_ picker: NSSharingServicePicker, didChoose service: NSSharingService?) {
        if service == nil { end() }
    }

    func sharingService(_ sharingService: NSSharingService, didShareItems items: [Any]) {
        end()
    }

    func sharingService(_ sharingService: NSSharingService, didFailToShareItems items: [Any], error: any Error) {
        end()
    }

    private func end() {
        ExportFolders.remove(url)
        Self.live.remove(self)
    }
}
#endif

/// An export's bytes, as a save panel writes them.
struct ExportedFile: FileDocument {
    static var readableContentTypes: [UTType] { [.pdf, .png, .zip] }
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
