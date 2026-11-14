import Foundation
@testable import ScaenaKit
import Testing

/// A theme read again as it was is parsed once (#1309's walk, hangs 35, 37, 28, and 30): the
/// inspector reads the theme on every edit and the layout picker on every draw, and a shipped
/// theme's 181 layouts parsed again each time held the window for seconds. A theme edited is
/// parsed anew, and the theme before it again once the edit is taken back.
@Test func aThemeReadAgainAsItWasIsParsedOnce() throws {
    let session = try ScaenaSession.create(theme: "Dusk", title: "Trail report")
    let text = try #require(try session.themeText()).text
    let parsing = ParsedTheme()

    let first = parsing.json(of: text)
    #expect(first.parsed)
    #expect((first.json?["layouts"]?.object?.count ?? 0) > 100)
    let again = parsing.json(of: text)
    #expect(!again.parsed, "read as it was: kept, not parsed again")
    #expect(again.json == first.json)

    let paper: [JSONValue] = [["op": "replace", "path": "/tokens/color/paper", "value": "#0B0B10"]]
    #expect(try session.themeEdit(paper).applied)
    let edited = try #require(try session.themeText()).text
    let after = parsing.json(of: edited)
    #expect(after.parsed, "edited: parsed anew")
    #expect(after.json?["tokens"]?["color"]?["paper"]?.string == "#0B0B10")
    #expect(parsing.json(of: text).parsed, "the theme before it, kept no more, is parsed again")

    // What the session's own read decodes is the theme as it is.
    #expect(try session.themeText()?.json?["tokens"]?["color"]?["paper"]?.string == "#0B0B10")
}
