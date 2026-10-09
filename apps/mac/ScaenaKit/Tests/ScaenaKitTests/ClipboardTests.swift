import CoreGraphics
import Foundation
import ImageIO
import ScaenaKit
import Testing
import UniformTypeIdentifiers

#if os(macOS)
import AppKit
#else
import UIKit
#endif

#if os(macOS)
/// What the pasteboard holds, as the canvas pastes it (PLAN 2.37, 2.96): a clip, written beside
/// its text; a picture, the words beside it only naming it; words, which outrank a picture of
/// them, as a sheet's cells come with one.
@MainActor
@Test func thePasteboardIsReadAsTheBrowsersClipboardIs() throws {
    let board = NSPasteboard.withUniqueName()
    defer { board.releaseGlobally() }
    #expect(Pasteboard.read(board) == nil)

    Pasteboard.write(clip: #"{"kind":"scaena/clip"}"#, to: board)
    #expect(Pasteboard.read(board) == .clip(#"{"kind":"scaena/clip"}"#))
    #expect(board.string(forType: .string) == #"{"kind":"scaena/clip"}"#, "beside it, its text")

    let png = try Data(contentsOf: repository.appending(path: "tests/fixtures/torture.scaena/assets/test-card.png"))
    board.clearContents()
    board.setData(png, forType: .png)
    #expect(Pasteboard.read(board) == .file(png, name: "picture.png"))

    // A sheet's cells, with a picture of them: the words.
    board.clearContents()
    board.setData(png, forType: .png)
    board.setString("Quarter\tSales\nQ1\t1,200", forType: .string)
    #expect(Pasteboard.read(board) == .words("Quarter\tSales\nQ1\t1,200"))

    // A picture of a kind an image does not show, as a PNG of it.
    let tiff = try #require(NSBitmapImageRep(data: png)?.tiffRepresentation)
    board.clearContents()
    board.setData(tiff, forType: .tiff)
    guard case .file(let made, let name) = Pasteboard.read(board) else {
        Issue.record("a TIFF reads as a picture")
        return
    }
    #expect(name == "picture.png" && made.starts(with: [0x89, 0x50, 0x4E, 0x47]))
}
#else
/// The iPad's pasteboard, read as the Mac's is (PLAN 4.1): a clip, written beside its text; a
/// picture, the words beside it only naming it; words, which outrank a picture of them; and a
/// picture of a kind an image does not show, as a PNG of it.
@MainActor
@Test func theIPadsPasteboardIsReadAsTheMacsIs() throws {
    let board = UIPasteboard.withUniqueName()
    defer { UIPasteboard.remove(withName: board.name) }
    #expect(Pasteboard.read(board) == nil)

    Pasteboard.write(clip: #"{"kind":"scaena/clip"}"#, to: board)
    #expect(Pasteboard.read(board) == .clip(#"{"kind":"scaena/clip"}"#))
    #expect(board.string == #"{"kind":"scaena/clip"}"#, "beside it, its text")

    let png = try Data(contentsOf: repository.appending(path: "tests/fixtures/torture.scaena/assets/test-card.png"))
    board.items = [[UTType.png.identifier: png]]
    #expect(Pasteboard.read(board) == .file(png, name: "picture.png"))

    // A sheet's cells, with a picture of them: the words.
    board.items = [[UTType.png.identifier: png, UTType.utf8PlainText.identifier: "Quarter\tSales\nQ1\t1,200"]]
    #expect(Pasteboard.read(board) == .words("Quarter\tSales\nQ1\t1,200"))

    // A picture of a kind an image does not show, as a PNG of it.
    let tiffed = try #require(tiff(png))
    board.items = [[UTType.tiff.identifier: tiffed]]
    guard case .file(let made, let name) = Pasteboard.read(board) else {
        Issue.record("a TIFF reads as a picture")
        return
    }
    #expect(name == "picture.png" && made.starts(with: [0x89, 0x50, 0x4E, 0x47]))
}

/// `png` written again as a TIFF, by ImageIO.
private func tiff(_ png: Data) -> Data? {
    guard let source = CGImageSourceCreateWithData(png as CFData, nil),
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else { return nil }
    let out = NSMutableData()
    guard let made = CGImageDestinationCreateWithData(out as CFMutableData, UTType.tiff.identifier as CFString, 1, nil)
    else { return nil }
    CGImageDestinationAddImage(made, image, nil)
    return CGImageDestinationFinalize(made) ? out as Data : nil
}
#endif

/// A drop's files, as the canvas takes them (PLAN 3.22): a picture or a data file the Finder or
/// Files dragged, by its name; a picture of a kind an image does not show, as a PNG of it; and
/// anything else, none.
@MainActor
@Test func aDroppedFileIsReadAsThePasteboardsIs() async throws {
    let png = try Data(contentsOf: repository.appending(path: "tests/fixtures/torture.scaena/assets/test-card.png"))
    let folder = FileManager.default.temporaryDirectory.appending(path: "drop-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: folder) }

    let card = folder.appending(path: "Test Card.png")
    try png.write(to: card)
    let cardItem = try #require(NSItemProvider(contentsOf: card))
    let picture = try #require(await Pasteboard.dropped(cardItem))
    #expect(picture.name == "Test Card.png", "named \(picture.name)")
    #expect(picture.data == png)

    let rows = folder.appending(path: "bars.csv")
    try Data("quarter,sales\nQ1,1200\n".utf8).write(to: rows)
    let rowsItem = try #require(NSItemProvider(contentsOf: rows))
    let data = try #require(await Pasteboard.dropped(rowsItem))
    #expect(data.name == "bars.csv", "named \(data.name)")
    #expect(String(decoding: data.data, as: UTF8.self).hasPrefix("quarter,sales"))

    // A TIFF's data, as a picture dragged from another app may be: a PNG of it.
    let tiff = try #require(retyped(png, as: .tiff))
    let item = NSItemProvider(item: tiff as NSData, typeIdentifier: UTType.tiff.identifier)
    let made = try #require(await Pasteboard.dropped(item))
    #expect(made.name.hasSuffix(".png"), "named \(made.name)")
    #expect(made.data.starts(with: [0x89, 0x50, 0x4E, 0x47]))

    // Words are no file the canvas takes.
    let words = folder.appending(path: "notes.txt")
    try Data("hello".utf8).write(to: words)
    let wordsItem = try #require(NSItemProvider(contentsOf: words))
    let taken = await Pasteboard.dropped(wordsItem)
    #expect(taken?.name == nil, "words are taken as \(taken?.name ?? "")")
}

/// `png` written again as `kind`, by ImageIO.
private func retyped(_ png: Data, as kind: UTType) -> Data? {
    guard let source = CGImageSourceCreateWithData(png as CFData, nil),
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else { return nil }
    let out = NSMutableData()
    guard let made = CGImageDestinationCreateWithData(out as CFMutableData, kind.identifier as CFString, 1, nil)
    else { return nil }
    CGImageDestinationAddImage(made, image, nil)
    return CGImageDestinationFinalize(made) ? out as Data : nil
}

/// The clipboard through the editor (PLAN 3.12), as the browser's (PLAN 2.37, 2.58, 2.96): a node
/// copied as a clip pasted in another state, a look put on another node, a sheet's cells pasted
/// as a source and the table they were, and a picture as an image; each patch made.
@Test func aClipALookCellsAndAPicturePasteAsTheBrowsersDo() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "goal"
    let session = editor.session

    let clip = try session.copying(state: "cover", nodes: ["title"])
    #expect(clip.contains("\"scaena/clip\""))
    let pasted = try session.pasting(clip, state: "goal", at: CGPoint(x: 960, y: 540))
    #expect(pasted.id != "title" && pasted.ids == [pasted.id] && pasted.lacked.isEmpty)
    try editor.make(pasted.patch)
    #expect(try session.boxes(state: "goal").contains { $0.node == pasted.id })
    let words = try session.pasting("Words from elsewhere", state: "goal", at: CGPoint(x: 100, y: 100))
    #expect(words.patch.first?["node"]?["type"]?.string == "text")

    let look = try session.look(state: "cover", node: "title")
    #expect(look["node"]?.string == "title")
    let put = try session.putting(state: "cover", look: look, nodes: ["subtitle"])
    #expect(!put.patch.isEmpty && put.took == ["subtitle"])
    try editor.make(put.patch)

    // A sheet's cells: a source, typed as they read, and the table they were.
    let cells = try #require(try session.cells("Quarter\tSales\nQ1\t1,200\nQ2\t1,450\n"))
    #expect(cells.rows == 2 && cells.columns == ["Quarter", "Sales"])
    #expect(try session.cells("just words") == nil)
    let path = try session.drop(Data(cells.csv.utf8), named: "\(cells.name).csv")
    #expect(path.hasPrefix("data/"))
    let attaching = try session.attaching(path: path, schema: cells.schema)
    try editor.make(attaching.patch)
    let offered = try session.inserts()
    let n = try #require(
        offered.firstIndex { $0.node["type"]?.string == "table" && $0.node["data"]?.string == "@\(attaching.data)" })
    let table = try session.inserting(
        state: "goal", n: n, at: CGPoint(x: 960, y: 700), with: ["columns": cells.tableColumns])
    try editor.make(table.patch)
    #expect(try session.boxes(state: "goal").contains { $0.node == table.id })

    // A picture, kept by its content, and inserted as an image.
    let png = try Data(contentsOf: repository.appending(path: "tests/fixtures/torture.scaena/assets/test-card.png"))
    let image = try session.drop(png, named: "Screenshot.png")
    #expect(image.hasPrefix("assets/") && image.hasSuffix(".png"))
    let m = try #require(
        try session.inserts().firstIndex { $0.node["type"]?.string == "image" && $0.node["src"]?.string == image })
    let added = try session.inserting(state: "goal", n: m, at: CGPoint(x: 400, y: 400), named: "Screenshot.png")
    try editor.make(added.patch)
    #expect(try session.boxes(state: "goal").contains { $0.node == added.id })
}
