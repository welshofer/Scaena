import Foundation
import ScaenaKit
import Testing

/// The torture deck (PLAN 0.2): data files and rows written inline, and images.
private let torture = repository.appending(path: "tests/fixtures/torture.scaena")

/// The theme panel (PLAN 3.15, as the browser's, PLAN 2.39, 2.61): the themes that ship, a deck put
/// in another and back; one whose layouts a theme lacks refused, with why; and the theme edited,
/// a dry run writing nothing, the file written before and after for the undo, which writes it back.
@Test func theThemeIsEditedAndAnotherTakenAsTheBrowsersThemeTabDoes() throws {
    let made = try ScaenaSession.create(theme: "Dusk", title: "Trail report")
    #expect(try made.shippedThemes().map(\.name) == ["dusk", "daybreak", "ember"])
    #expect(try made.themes().current == "themes/dusk.theme.json")
    let themed = try made.retheme(ships: "daybreak")
    #expect(themed.applied && !themed.refused && themed.theme == "themes/daybreak.theme.json")
    #expect(try made.themes().files.contains("themes/dusk.theme.json"))
    #expect(try made.retheme(path: "themes/dusk.theme.json").applied)

    let session = try ScaenaSession(directory: b1)
    let refused = try session.retheme(ships: "daybreak")
    #expect(refused.refused && !refused.applied && refused.why.contains { $0.contains("layout") })
    #expect(try session.themes().current == "theme.json")

    let paper: [JSONValue] = [["op": "replace", "path": "/tokens/color/paper", "value": "#0B0B10"]]
    #expect(try session.themeEdit(paper, dryRun: true).files.isEmpty)
    let edited = try session.themeEdit(paper)
    #expect(edited.applied && edited.files.first?.path == "theme.json")
    #expect(edited.files.first?.after?.contains("#0B0B10") == true)
    let color = { () throws -> String? in try session.themeText()?.json?["tokens"]?["color"]?["paper"]?.string }
    #expect(try color() == "#0B0B10")
    try session.write(edited.files.map { (path: $0.path, text: $0.before) })
    #expect(try color() == "#101014", "undone: the theme written back")
}

/// The data and files panels (PLAN 3.15, as the browser's, PLAN 2.55, 2.59): a source as a sheet,
/// a cell set, undone and redone; a row added and taken away; a value its column does not read
/// refused; and a file nothing names taken out and put back, one the deck names kept.
@Test func dataIsEditedAndFilesTakenOutAsTheBrowsersPanelsDo() throws {
    let session = try ScaenaSession(directory: torture)
    let sources = try session.dataSources()
    #expect(sources.contains { $0.name == "bars" && $0.file == "data/bars.csv" })
    #expect(sources.contains { $0.name == "rev" && $0.file == nil })
    let sheet = try session.dataSheet("bars")
    #expect(sheet.file == "data/bars.csv" && sheet.columns.map(\.name) == ["label", "value"])
    #expect(sheet.rows.first == ["2025-Q1", "12"] && sheet.rows.count == 6)

    let edited = try session.dataEdit("bars", [.set(row: 0, column: "value", value: "99")])
    #expect(edited.edited && edited.wrote && edited.file == "data/bars.csv")
    let first = { () throws -> String? in try session.dataSheet("bars").rows.first?.last }
    #expect(try first() == "99")
    #expect(try session.dataUndo() == "bars")
    #expect(try first() == "12")
    #expect(try session.dataUndo(redo: true) == "bars")
    #expect(try first() == "99")

    try session.dataEdit("bars", [.add(row: nil, values: ["label": "2026-Q3", "value": "5"])])
    #expect(try session.dataSheet("bars").rows.last == ["2026-Q3", "5"])
    try session.dataEdit("bars", [.remove(row: 6)])
    #expect(try session.dataSheet("bars").rows.count == 6)
    #expect(throws: ScaenaError.self) { try session.dataEdit("bars", [.set(row: 0, column: "value", value: "lots")]) }
    let inline = try session.dataEdit("rev", [.set(row: 0, column: "rev", value: "13")])
    #expect(inline.wrote && inline.file == nil, "rows written inline are the deck's")

    let path = try session.drop(Data("a,b\n1,2\n".utf8), named: "extra.csv")
    #expect(try session.bundleFiles().contains { $0.path == path && $0.named.isEmpty })
    try session.removeFile(path)
    #expect(try !session.bundleFiles().contains { $0.path == path })
    #expect(try session.dataUndo() == path)
    #expect(try session.bundleFiles().contains { $0.path == path })
    #expect(throws: ScaenaError.self) { try session.removeFile("data/bars.csv") }
    let bars = try #require(try session.bundleFiles().first { $0.path == "data/bars.csv" })
    #expect(bars.type == "data" && bars.named == ["the data source bars"])
}

/// The versions panel (PLAN 3.15, as the browser's, PLAN 2.60): two saves keep two versions, read
/// from the bundle's history; the first shown and drawn, compared with the deck now, and made the
/// deck again.
@Test func versionsAreListedShownComparedAndRestoredFromTheHistory() throws {
    let session = try ScaenaSession(directory: b1)
    #expect(throws: ScaenaError.self) { try session.versions() }
    try session.keepHistory()
    #expect(try session.keepsHistory())
    let editor = DeckEditor(session: session)
    try session.adopt(try session.save(at: Date(timeIntervalSince1970: 1_790_000_000), subset: false))
    try editor.typed([["op": "replace_text", "node": "title", "state": "cover", "from": 6, "to": 6, "text": " anew"]])
    try session.adopt(try session.save(at: Date(timeIntervalSince1970: 1_790_000_300), subset: false))

    let versions = try session.versions()
    #expect(versions.map(\.n) == [1, 2])
    #expect(try session.viewVersion("1").first == "cover")
    let png = try session.versionPNG("cover", width: 320)
    #expect(png.starts(with: [0x89, 0x50, 0x4E, 0x47]))
    let compared = try session.compareVersions(from: "1")
    #expect(compared.states.contains { $0.state == "cover" && $0.how == "changed" })
    let restored = try session.restoreVersion("1")
    #expect(restored.applied && restored.why.isEmpty)
    #expect(try !session.source().contains("anew"))
    #expect(throws: ScaenaError.self) { try session.viewVersion("9") }
}

/// A panel's number field (PLAN 3.28): a number shown as a person writes it, read from what they
/// typed, and stepped without the noise of a binary fraction.
@Test func aFieldShowsReadsAndStepsANumberAsAPersonWritesIt() {
    #expect(FieldNumber.shown(32) == "32")
    #expect(FieldNumber.shown(1.25) == "1.25")
    #expect(FieldNumber.shown(-0.02) == "-0.02")
    #expect(FieldNumber.shown(0.1 + 0.2) == "0.3")
    #expect(FieldNumber.shown(-0.0) == "0")
    #expect(FieldNumber.shown(nil) == "")
    #expect(FieldNumber.read(" 40 ") == 40)
    #expect(FieldNumber.read("1,5") == 1.5)
    #expect(FieldNumber.read("-0.02") == -0.02)
    #expect(FieldNumber.read("big") == nil)
    #expect(FieldNumber.read("") == nil)
    #expect(FieldNumber.read("inf") == nil)
    #expect(FieldNumber.stepped(1.2, by: 0.05) == 1.25)
    #expect(FieldNumber.stepped(1.25, by: -0.05) == 1.2)
    #expect(FieldNumber.stepped(-0.02, by: 0.01) == -0.01)
    #expect(FieldNumber.stepped(-0.025, by: 0.01) == -0.015)
    #expect(FieldNumber.stepped(0.92, by: 0.05) == 0.97)
    #expect(FieldNumber.stepped(333, by: 100) == 433)
    #expect(FieldNumber.shown(FieldNumber.stepped(0.1, by: 0.2)) == "0.3")
}
