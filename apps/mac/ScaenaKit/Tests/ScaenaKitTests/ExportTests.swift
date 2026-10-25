import Foundation
import ScaenaKit
import Testing

/// The deck's exports (PLAN 3.8): the PDF `scaena export` writes, which the app shares, shows in
/// Quick Look, and saves; and a state at rest as a PNG the width asked.
@Test func theDeckExportsAsTheCLIWritesIt() throws {
    let session = try ScaenaSession(directory: b1)
    let pdf = try session.pdf()
    #expect(pdf.starts(with: Array("%PDF-".utf8)), "a PDF")
    let png = try session.png("cover", width: 640)
    #expect(png.starts(with: [0x89, 0x50, 0x4E, 0x47]), "a PNG")
    let width = png[16..<20].reduce(0) { $0 << 8 | Int($1) }
    #expect(width == 640)
    #expect(throws: ScaenaError.self) { try session.png("cover", width: 0) }
    #expect(throws: ScaenaError.self) { try session.png("no-such-state", width: 100) }
}

/// Every slide as PNGs in one zip, each named by its number (PLAN 2.122), as Share offers it.
@Test func everySlideExportsAsPNGsInOneZip() throws {
    let session = try ScaenaSession(directory: b1)
    let zipped = try session.slides(width: 320, name: "B1")
    #expect(zipped.starts(with: [0x50, 0x4B, 0x03, 0x04]), "a zip")
    #expect(zipped.range(of: Data("B1 - Slide 1.png".utf8)) != nil, "its first slide, by its number")
    #expect(throws: ScaenaError.self) { try session.slides(width: 0, name: "B1") }
}

/// An export for the Share sheet or Quick Look is written into a folder of its own, taken away
/// once it is shared or seen, and what a quit left is swept as the app starts once it is older than
/// a day (lift-up REL-5); nothing else in the folder is touched.
@Test func anExportsFolderIsTakenAwayOnceDoneAndOldOnesAreSwept() throws {
    let room = FileManager.default.temporaryDirectory.appending(path: "exports-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: room, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: room) }
    let other = room.appending(path: "Kept")
    try FileManager.default.createDirectory(at: other, withIntermediateDirectories: true)

    let shared = try ExportFolders.write(Data("%PDF-".utf8), named: "Deck.pdf", in: room)
    #expect(try Data(contentsOf: shared) == Data("%PDF-".utf8))
    #expect(shared.deletingLastPathComponent().lastPathComponent.hasPrefix(ExportFolders.prefix))
    ExportFolders.remove(shared)
    #expect(!FileManager.default.fileExists(atPath: shared.deletingLastPathComponent().path))
    // A URL in no export folder takes nothing away.
    ExportFolders.remove(other.appending(path: "Deck.pdf"))
    #expect(FileManager.default.fileExists(atPath: other.path))

    let left = try ExportFolders.write(Data([1]), named: "Deck - Slides.zip", in: room)
    #expect(ExportFolders.sweep(now: Date(), in: room) == 0, "a folder made just now is kept")
    #expect(ExportFolders.sweep(now: Date().addingTimeInterval(2 * 24 * 60 * 60), in: room) == 1)
    #expect(!FileManager.default.fileExists(atPath: left.deletingLastPathComponent().path))
    #expect(FileManager.default.fileExists(atPath: other.path), "a folder of another name is kept")
}

/// An export is made away from the window, on a snapshot's session of its own, byte for byte what
/// the session makes; an edit after the snapshot is not in it, and a task cancelled meanwhile
/// throws, its bytes dropped (lift-up PERF-3).
@Test func anExportIsMadeAwayFromTheWindowAsTheSessionMakesIt() async throws {
    let session = try ScaenaSession(directory: b1)
    let snapshot = try session.snapshot()
    #expect(try await snapshot.exported(.pdf) == session.pdf())
    #expect(try await snapshot.exported(.png(state: "cover", width: 320)) == session.png("cover", width: 320))
    #expect(try await snapshot.exported(.slides(width: 160, name: "B1")) == session.slides(width: 160, name: "B1"))
    let state = try session.states()[0]
    let inspected: JSONValue = try session.call("inspect", ["state": .string(state)])
    let node = try #require(inspected["looks"]?.object?.keys.sorted().first)
    let op: JSONValue = [
        "op": "replace_text", "state": .string(state), "node": .string(node), "from": 0, "to": 0, "text": "Later. ",
    ]
    #expect(try session.tool("deck_patch", ["ops": [op]]).edited)
    #expect(try await snapshot.exported(.png(state: state, width: 320)) != session.png(state, width: 320))
    let cancelled = Task { try await snapshot.exported(.pdf) }
    cancelled.cancel()
    await #expect(throws: CancellationError.self) { try await cancelled.value }
}
