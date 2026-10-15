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
    #expect(try made.shippedThemes().map(\.name) == ["dusk", "daybreak", "ember", "gallery"])
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

/// A change the deck refuses is said as the window says it (PLAN 3.18), as the browser's `finding`
/// says it: what lint found, in a sentence, by the object it is about or the deck; never lint's own
/// message, which names what it finds by the ids the deck writes.
@Test func aChangeRefusedIsSaidInAPersonsWords() throws {
    let session = try ScaenaSession(directory: b1)
    let refused = try session.retheme(ships: "daybreak")
    let found = try #require(refused.refusals.first)
    let said = DeckEditor(session: session).refusal(found)
    #expect(said.hasSuffix(".") && !said.contains("`"), "\(said)")
    #expect(!refused.why.contains { said.contains($0) }, "\(said)")
}

/// A theme taken recolors what it would leave too faint to read (PLAN 2.82), as the browser's
/// does: trails in Daybreak sets its storm slide's title and note in the theme's color that reads
/// on the photo, and the panel says how many. A deck with nothing to recolor recolors nothing.
@Test func aThemeTakenRecolorsWhatItWouldLeaveTooFaintToRead() throws {
    let trails = try ScaenaSession(files: example("trails"))
    let themed = try trails.retheme(ships: "daybreak")
    #expect(themed.applied && themed.recolored == 2, "\(themed.recolored) recolored")
    let made = try ScaenaSession.create(theme: "Dusk", title: "Trail report")
    #expect(try made.retheme(ships: "daybreak").recolored == 0)
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
    // Each change in a person's words, as the browser's Versions says it (PLAN 2.99): no
    // operation's name or author's id reaches the panel.
    #expect(versions[0].said == "The history begins" && versions[0].by == "you")
    #expect(!versions[1].said.isEmpty && !versions[1].said.contains("patch:") && !versions[1].said.contains("_"))
    let states = try session.viewVersion("1")
    #expect(states.first == "cover")
    // Its slides as its own slide list numbers them.
    let slides = try session.versionSlides()
    #expect(slides.count == states.count)
    #expect(Array(slides.prefix(3)) == ["Slide 1", "Slide 2", "Slide 2, step 2"])
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

/// The inspector's swatches (PLAN 3.18): two colors a person cannot tell apart at a swatch's size
/// are one swatch, as Dusk's two near-black backgrounds are; its line color, darker than the text
/// and lighter than the backgrounds, is a swatch of its own.
@Test func colorsThatLookAlikeInASwatchAreOne() {
    #expect(Swatch.looksAlike("#101014", "#17171D"))
    #expect(Swatch.looksAlike("#17171d", "#17171D"), "as the theme writes it, either case")
    #expect(!Swatch.looksAlike("#101014", "#2A2A33"))
    #expect(!Swatch.looksAlike("#17171D", "#2A2A33"))
    #expect(!Swatch.looksAlike("#FF6A3D", "#FFC857"))
    #expect(Swatch.looksAlike("#101014", "#101014FF"), "opaque either way")
    #expect(!Swatch.looksAlike("#101014", "#10101480"), "half as opaque")
    #expect(Swatch.looksAlike("oklch(0.5 0.1 30)", "OKLCH(0.5 0.1 30)"))
    #expect(!Swatch.looksAlike("oklch(0.5 0.1 30)", "#808080"))
}
