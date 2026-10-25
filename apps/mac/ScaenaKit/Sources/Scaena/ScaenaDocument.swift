import Foundation
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers
import os

extension UTType {
    /// A Scaena bundle: a folder the Finder shows as one file, holding `deck.json`, its theme,
    /// fonts, images, data, and its history (SPEC §3.1). A `.scaena` zip opens too, and is
    /// saved as the folder.
    static let scaenaDeck = UTType(exportedAs: "com.scaena.deck", conformingTo: .package)

    /// A Scaena bundle zipped, as `scaena save` and the browser's download write one. It shares
    /// the `.scaena` extension: macOS gives a folder `scaenaDeck` and a file this, as Keynote's
    /// `.key` is a package or a flat file.
    static let scaenaDeckZip = UTType(exportedAs: "com.scaena.deck.zip", conformingTo: .zip)
}

/// A deck open in the app: the session the browser edits (ADR-0021), edited as the browser's
/// editor edits it (`DeckEditor`, PLAN 3.4), and saved as `scaena save` lays a bundle out, with
/// each edit since the last save recorded in its history (PLAN 3.3, SPEC §8). macOS keeps its
/// versions, as it does any document's that saves in place.
final class ScaenaDocument: ReferenceFileDocument {
    /// A zip not named `.scaena` is read too, from Open With, the Open dialog, and a drop; the app
    /// owns only the two types it defines, and is an alternate for zips (Info.plist, lift-up USE-1).
    static var readableContentTypes: [UTType] { [.scaenaDeck, .scaenaDeckZip, .zip] }
    static var writableContentTypes: [UTType] { [.scaenaDeck] }

    let editor: DeckEditor
    var session: ScaenaSession { editor.session }
    /// Whether the deck was made here by New, not opened: its window offers the themes that ship
    /// before anything is on it, as the browser's New does (PLAN 2.12), once.
    var startsNew = false
    /// What its window says as it opens, once: what a deck made from a prompt came to (PLAN G.6).
    var opening: String?
    /// Why New made the deck in another theme than Dusk, which its window says once as it opens
    /// (lift-up STAB-3).
    var fellBack: String?

    /// A new deck, as New starts one in the browser: Dusk, one state with nothing on it. Where Dusk
    /// makes none, the next theme that ships does, and the window says why (lift-up STAB-3).
    init() {
        let (made, failed) = ScaenaSession.create(title: "Untitled")
        for (theme, error) in failed {
            openingLog.error("New: \(theme, privacy: .public) made no deck: \(String(describing: error), privacy: .public)")
        }
        guard let session = made else {
            // Every theme the app carries made no deck: the engine itself is broken, and there is
            // no deck to open a window on.
            let why = failed.map { "\($0.theme): \($0.error)" }.joined(separator: "; ")
            preconditionFailure("no theme that ships made a deck: \(why)")
        }
        if let first = failed.first, let used = ScaenaSession.newThemes.dropFirst(failed.count).first {
            fellBack = "\(first.theme.capitalized) couldn’t make a new deck, so it was made in "
                + "\(used.capitalized): \(Why.said(first.error))"
        }
        _ = try? session.call("keepHistory") as JSONValue
        editor = DeckEditor(session: session)
        // It opens on a title slide, its words to type over, as a presentation app's does (PLAN 3.30).
        editor.startOnTitleSlide()
        startsNew = true
    }

    /// A deck made from a prompt (PLAN G.6): the session the generator opened, kept nowhere until
    /// it is saved, its window saying `said`, what it came to, as it opens.
    init(generated session: ScaenaSession, said: String) {
        _ = try? session.call("keepHistory") as JSONValue
        editor = DeckEditor(session: session)
        opening = said
    }

    init(configuration: ReadConfiguration) throws {
        let name = configuration.file.filename ?? "a deck"
        openingLog.debug("opening \(name, privacy: .public)")
        let began = Date()
        let session: ScaenaSession
        do {
            session = try Self.open(configuration.file)
        } catch {
            openingLog.debug("\(name, privacy: .public) did not open: \(String(describing: error), privacy: .public)")
            throw error
        }
        // Each save records the edits since in the bundle's history, which the first begins.
        _ = try? session.call("keepHistory") as JSONValue
        editor = DeckEditor(session: session)
        #if !os(macOS)
        if let file = configuration.file.filename {
            opening = Self.handed.withLock { handed -> String? in
                guard let given = handed, file.hasPrefix(given.name) else { return nil }
                handed = nil
                return given.said
            }
        }
        #endif
        openingLog.debug("\(name, privacy: .public) opened in \(Date().timeIntervalSince(began)) s")
    }

    #if !os(macOS)
    /// A deck New from a Prompt handed the iPad's document browser (PLAN G.6): the name it was
    /// written under, and what it says as it opens. The browser opens it as a new document under
    /// that name, or that name numbered where the folder holds one already.
    private struct Handed: Sendable {
        let name: String
        let said: String
    }

    private static let handed = OSAllocatedUnfairLock<Handed?>(initialState: nil)

    /// The deck written as `name` is to say `said` as it opens.
    static func handing(_ said: String, as name: String) {
        handed.withLock { $0 = Handed(name: name, said: said) }
    }
    #endif

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

    /// What a save reads of the session, taken on the main actor as SwiftUI asks for it, and the
    /// session to go on from the save (lift-up PERF-5). Sendable: the session is touched only back
    /// on the main actor.
    struct Taken: @unchecked Sendable {
        let snapshot: ScaenaSession.Snapshot
        let session: ScaenaSession
        /// What the window says what the save said of the history through (lift-up 20261025b REL-1).
        let editor: DeckEditor
    }

    /// The bundle as it stands, taken in a moment: what is slow in a save is done off the main
    /// actor, in `fileWrapper` (lift-up PERF-5). What is saved is the deck shown: a source typed
    /// that does not compile is not the deck yet.
    func snapshot(contentType: UTType) throws -> Taken {
        Taken(snapshot: try session.snapshot(), session: session, editor: editor)
    }

    /// The bundle saved, off the main actor, as SwiftUI writes it: its files by their paths, fonts
    /// subset and the history recorded. The session goes on from the save, which names files by
    /// their content, back on the main actor, where it is still as it was taken: an edit made
    /// meanwhile is kept, and the next save names its files again (lift-up PERF-5).
    func fileWrapper(snapshot taken: Taken, configuration: WriteConfiguration) throws -> FileWrapper {
        let saved = try taken.snapshot.save(subset: true)
        var files: [String: Data] = [:]
        for path in saved.files {
            files[path] = saved.file(path)
        }
        let wrapper = Self.wrapper(files)
        DispatchQueue.main.async {
            // A save not gone on from is one the next save makes again: nothing is lost.
            _ = try? taken.session.adopt(saved, since: taken.snapshot)
            // A history that could not be read, or a newer Scaena's, said once (REL-1).
            if let said = DeckEditor.said(ofSave: saved.summary) { taken.editor.savedSaid = said }
        }
        return wrapper
    }

    /// The bundle saved now, on the caller's thread, and the session gone on from it: what a deck
    /// made from a prompt is written as once, before any window shows it (PLAN G.6).
    func savedNow() throws -> [String: Data] {
        let saved = try session.save(subset: true)
        var files: [String: Data] = [:]
        for path in saved.files {
            files[path] = saved.file(path)
        }
        try session.adopt(saved)
        return files
    }

    /// A bundle's files, by their paths in it, as the folder that holds them.
    static func wrapper(_ snapshot: [String: Data]) -> FileWrapper {
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
    /// undo, which makes the deck its source again, named in the Edit menu as `named` says, or as
    /// the patch says what it does (PLAN 3.28, 4.11): Undo Move, Redo Duplicate.
    func make(_ ops: [JSONValue], undo: UndoManager?, named: String? = nil) throws {
        let before = try editor.make(ops)
        keep(before, undo: undo)
        undo?.setActionName(named ?? StepName.of(ops, label: Words.label))
    }

    /// Compile `typed`, the source pane's text: where it becomes the deck, one step to undo.
    func type(_ typed: String, undo: UndoManager?) {
        guard let before = editor.type(typed) else { return }
        keep(before, undo: undo)
        undo?.setActionName("Typing")
    }

    /// Apply `finding`'s fix: one step to undo.
    func fix(_ finding: Finding, undo: UndoManager?) throws {
        let before = try editor.fix(finding)
        keep(before, undo: undo)
        undo?.setActionName("Fix")
    }

    /// Take text typed in place on the canvas (PLAN 3.9): `before`, the source the change replaced.
    /// A change that joins the burst of typing before it adds no step: the burst's first change
    /// keeps the source from before it, so one undo takes the whole burst back.
    func typedInPlace(_ before: String, joins: Bool, undo: UndoManager?) {
        guard !joins else { return }
        keep(before, undo: undo)
        undo?.setActionName("Typing")
    }

    /// Take an edit the assistant made on the deck (PLAN 3.6): `before`, the source it replaced,
    /// and the files it wrote beside the deck, the theme `theme_edit` edited. One step to undo,
    /// as the user's own edit is: the source made the deck again, and the files written back.
    func took(_ before: String, files: [Rewritten], undo: UndoManager?) {
        keep(before, files: files, undo: undo)
        undo?.setActionName("Assistant’s Edit")
    }

    /// An edit a panel makes on the session beside the source (PLAN 3.15): the theme edited or
    /// another taken, a version restored, rows written inline. The deck is read again, and the
    /// edit is one step to undo, which makes the source as it was and writes back the files `edit`
    /// gives as they were.
    func beside(_ edit: () throws -> [Rewritten], undo: UndoManager?) throws {
        let before = editor.source
        let files = try edit()
        editor.reread()
        keep(before, files: files, undo: undo)
    }

    /// A data file a panel writes, or a file it takes out (PLAN 3.15): the deck read again, and
    /// one step to undo, the session's own, which puts the file back as it was, and the source as
    /// it was where the edit set a quote of the data again; redone, it is written again.
    func data(_ edit: () throws -> Void, undo: UndoManager?) throws {
        let before = editor.source
        try edit()
        editor.reread()
        dataStep(redo: false, undo: undo)
        if editor.source != before { keep(before, undo: undo) }
    }

    /// Register the session's data undo, or its redo, as the step to undo; done, the other.
    private func dataStep(redo: Bool, undo: UndoManager?) {
        undo?.registerUndo(withTarget: self) { [weak undo] document in
            _ = try? document.session.dataUndo(redo: redo)
            document.editor.reread()
            document.dataStep(redo: !redo, undo: undo)
        }
    }

    /// Register making the deck `source` again, `files` written back first, as the step to undo;
    /// undone, its redo.
    private func keep(_ source: String, files: [Rewritten] = [], undo: UndoManager?) {
        undo?.registerUndo(withTarget: self) { [weak undo] document in
            if !files.isEmpty {
                try? document.session.write(files.map { (path: $0.path, text: $0.before) })
            }
            guard let now = document.editor.restore(source) else { return }
            document.keep(now, files: files.map(\.undone), undo: undo)
        }
    }
}
