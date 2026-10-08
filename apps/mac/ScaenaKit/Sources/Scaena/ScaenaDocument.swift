import Foundation
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    /// A Scaena bundle: a folder the Finder shows as one file, holding `deck.json`, its theme,
    /// fonts, images, data, and its history (SPEC §3.1). A `.scaena` zip opens too, and is
    /// saved as the folder.
    static let scaenaDeck = UTType(exportedAs: "com.scaena.deck", conformingTo: .package)
}

/// A deck open in the app: the session the browser edits (ADR-0021), saved as `scaena save` lays
/// a bundle out, with each edit since the last save recorded in its history (PLAN 3.3, SPEC
/// §8). macOS keeps its versions, as it does any document's that saves in place.
final class ScaenaDocument: ReferenceFileDocument {
    static var readableContentTypes: [UTType] { [.scaenaDeck, .zip] }
    static var writableContentTypes: [UTType] { [.scaenaDeck] }

    let session: ScaenaSession
    /// Bumped by each edit, so the window draws the deck again.
    @Published var revision = 0

    /// A new deck, as New starts one in the browser: Dusk, one state with nothing on it.
    init() {
        do {
            session = try ScaenaSession.create(theme: "dusk", title: "Untitled")
        } catch {
            // Dusk ships inside the app: a deck made from it always validates.
            fatalError("the theme that ships made no deck: \(error)")
        }
        _ = try? session.call("keepHistory") as JSONValue
    }

    init(configuration: ReadConfiguration) throws {
        session = try Self.open(configuration.file)
        // Each save records the edits since in the bundle's history, which the first begins.
        _ = try? session.call("keepHistory") as JSONValue
    }

    /// The bundle `file` holds: a folder's files by their paths in it, or a zip.
    static func open(_ file: FileWrapper) throws -> ScaenaSession {
        guard file.isDirectory else {
            guard let zip = file.regularFileContents else { throw CocoaError(.fileReadCorruptFile) }
            return try ScaenaSession(zip: zip)
        }
        var files: [String: Data] = [:]
        func walk(_ folder: FileWrapper, _ prefix: String) {
            for (name, child) in folder.fileWrappers ?? [:] where !name.hasPrefix(".") {
                let path = prefix.isEmpty ? name : "\(prefix)/\(name)"
                if child.isDirectory {
                    walk(child, path)
                } else if let data = child.regularFileContents {
                    files[path] = data
                }
            }
        }
        walk(file, "")
        return try ScaenaSession(files: files)
    }

    /// The bundle saved now: its files by their paths, fonts subset and the history recorded. The
    /// session goes on from the save, which names files by their content.
    func snapshot(contentType: UTType) throws -> [String: Data] {
        let saved = try session.save(subset: true)
        var files: [String: Data] = [:]
        for path in saved.files {
            files[path] = saved.file(path)
        }
        try session.adopt(saved)
        return files
    }

    func fileWrapper(snapshot: [String: Data], configuration: WriteConfiguration) throws -> FileWrapper {
        let root = FileWrapper(directoryWithFileWrappers: [:])
        for (path, data) in snapshot.sorted(by: { $0.key < $1.key }) {
            var folder = root
            let parts = path.split(separator: "/").map(String.init)
            for part in parts.dropLast() {
                if let child = folder.fileWrappers?[part], child.isDirectory {
                    folder = child
                } else {
                    let child = FileWrapper(directoryWithFileWrappers: [:])
                    child.preferredFilename = part
                    _ = folder.addFileWrapper(child)
                    folder = child
                }
            }
            let file = FileWrapper(regularFileWithContents: data)
            file.preferredFilename = parts.last
            _ = folder.addFileWrapper(file)
        }
        return root
    }

    /// Make `ops`, a patch, as the user, as the browser's gestures do (ADR-0013): one step to
    /// undo, which puts the deck's source back.
    func make(_ ops: JSONValue, undo: UndoManager?) throws {
        let before = try session.source()
        let called = try session.tool("deck_patch", ["ops": ops])
        guard called.edited else { return }
        revision += 1
        undo?.registerUndo(withTarget: self) { $0.restore(before, undo: undo) }
    }

    /// The deck `source` compiles to, made the deck again: an undo, or its redo.
    private func restore(_ source: String, undo: UndoManager?) {
        guard let now = try? session.source() else { return }
        _ = try? session.compile(source)
        revision += 1
        undo?.registerUndo(withTarget: self) { $0.restore(now, undo: undo) }
    }
}
