import Foundation
@testable import ScaenaKit
import Testing

/// A theme edit lints the whole deck in the theme it leaves and the one it makes. Made on the
/// window's thread, it kept the Mac's window from answering, a person and VoiceOver alike, for
/// seconds, and for minutes on CI's Mac (#1262). It is made away from the window: off the main
/// thread, then the theme it wrote is written into the session, which draws in it from then on.
@Test @MainActor func aThemeEditIsMadeOffTheMainThreadAndTheSessionDrawsInIt() async throws {
    let window = Window(editor: DeckEditor(session: try ScaenaSession(directory: b1)))
    let editor = window.editor
    let paper = { () throws -> String? in
        try editor.session.themeText()?.json?["tokens"]?["color"]?["paper"]?.string
    }
    #expect(try paper() == "#101014")
    let digest = try editor.session.digest(state: "cover")

    let themedOn = ThemedOn()
    editor.themedOn = { themedOn.take(main: $0) }
    let away = try await editor.themeEditAway(.ops(AThemeEdit.paper("#2B1B3D")))
    let made = try #require(away, "the theme edit was made")
    editor.themedOn = nil
    let main = try #require(themedOn.taken as Bool?, "the theme edit ran")
    #expect(!main, "made on the main thread")

    #expect(made.edited.applied && made.source == nil, "a color edits the theme alone")
    let file = try #require(made.edited.files.first)
    #expect(file.path == "theme.json" && file.after?.contains("#2B1B3D") == true)
    #expect(try paper() == "#2B1B3D", "the session's theme is the one made")
    #expect(try editor.session.digest(state: "cover") != digest, "the cover drawn in the theme made")

    // Read again with no lint of the state shown, which would paint what lies behind each of its
    // texts on the window's thread (#1262's sample): every state is linted away from it after.
    let revision = editor.revision
    editor.reread(lint: false)
    #expect(editor.revision == revision + 1 && editor.valid, "the deck compiled in the theme made")
    #expect(!editor.whole, "its lint of every state waits for lintEvery")
    await editor.lintEvery()
    #expect(editor.whole, "linted away from the window")

    // Its undo writes the theme back, as a theme edit made on the window's thread had it.
    try editor.session.write(made.edited.files.map { (path: $0.path, text: $0.before) })
    #expect(try paper() == "#101014")
}

/// A deck changed while a theme edit is made (#1262): the edit is made again on the deck as it then
/// stands, so neither the change made meanwhile nor the theme edit is lost; a refused one writes
/// nothing.
@Test @MainActor func aThemeEditOutrunByAnotherEditIsMadeAgainOnTheDeckAsItStands() async throws {
    let window = Window(editor: DeckEditor(session: try ScaenaSession(directory: b1)))
    let editor = window.editor
    editor.shown = "cover"
    let typed: JSONValue = [
        "op": "replace_text", "state": "cover", "node": "title", "from": 0, "to": 0, "text": "Edited. ",
    ]
    // Made once the theme edit has taken the deck, on the window's actor, before it waits for the
    // edit: under way, with no clock to catch it by. Once only: the edit made again is not outrun.
    var tries = 0
    var edited: Result<String, any Error>?
    editor.themeEditing = {
        tries += 1
        if tries == 1 { edited = Result { try editor.make([typed]) } }
    }
    let away = try await editor.themeEditAway(.ops(AThemeEdit.paper("#2B1B3D")))
    let made = try #require(away, "the theme edit was made")
    editor.themeEditing = nil
    _ = try #require(edited, "the deck was changed while the theme edit was made").get()
    #expect(tries == 2, "made again once the deck changed: \(tries) tries")
    #expect(made.edited.applied)
    #expect(try editor.session.themeText()?.json?["tokens"]?["color"]?["paper"]?.string == "#2B1B3D")
    #expect(try editor.session.source().contains("Edited. "), "the edit made meanwhile kept")

    // A theme the deck would not open in, one without a layout a slide takes, is refused: nothing
    // written.
    let before = try editor.session.themeText()?.text
    let broken: [JSONValue] = [["op": "remove", "path": "/layouts/belief"]]
    let refusing = try await editor.themeEditAway(.ops(broken))
    let refused = try #require(refusing, "the theme edit was answered")
    #expect(!refused.edited.applied && refused.edited.refused)
    #expect(try editor.session.themeText()?.text == before, "a refused edit writes nothing")
}

/// A re-theme lints the whole deck in the theme it leaves and the one it makes, as a theme edit does,
/// and made on the window's thread kept it from answering as one did (#1262's follow-up). It is made
/// away from the window, on a session opened on a snapshot, and the session takes the deck it made
/// and the files it wrote, the theme's fonts among them; an edit made meanwhile has it made again.
@Test @MainActor func aRethemeIsMadeOffTheMainThreadAndTheSessionTakesItsFonts() async throws {
    // A new deck in Dusk, on its title slide, as File › New opens one: every layout it takes is
    // Ember's too.
    let window = Window(editor: DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Trail report")))
    let editor = window.editor
    let shown = try #require(editor.startOnTitleSlide(), "the deck starts on its title slide")
    editor.shown = shown
    let digest = try editor.session.digest(state: shown)
    let fonts = { try editor.session.call("files", as: [String].self).filter { $0.hasPrefix("fonts/") } }
    let before = try fonts()

    let themedOn = ThemedOn()
    editor.themedOn = { themedOn.take(main: $0) }
    var tries = 0
    let noted: JSONValue = ["op": "set_state", "id": .string(shown), "prop": "notes", "value": "Said meanwhile."]
    var edited: Result<String, any Error>?
    editor.themeEditing = {
        tries += 1
        if tries == 1 { edited = Result { try editor.make([noted]) } }
    }
    let themed = try #require(try await editor.rethemeAway(.ships("Ember")), "the re-theme was made")
    editor.themeEditing = nil
    editor.themedOn = nil
    #expect(try #require(themedOn.taken as Bool?, "the re-theme ran") == false, "made on the main thread")
    _ = try #require(edited, "the deck was changed while the re-theme was made").get()
    #expect(tries == 2, "made again once the deck changed: \(tries) tries")

    #expect(themed.applied && themed.theme == "themes/ember.theme.json")
    #expect(try editor.session.themeText()?.theme == "themes/ember.theme.json", "the session's theme is Ember")
    #expect(try fonts() != before, "Ember's fonts taken with it")
    #expect(try editor.session.source().contains("Said meanwhile."), "the edit made meanwhile kept")
    #expect(try editor.session.digest(state: shown) != digest, "the title slide drawn in Ember")

    // Read again with no lint of the state shown: every state is linted away from the window after.
    editor.reread(lint: false)
    #expect(editor.valid && !editor.whole)
    await editor.lintEvery()
    #expect(editor.whole, "linted away from the window")

    // A theme the deck would not open in is refused, nothing taken: B1 lays slides out in layouts
    // Ember has not.
    let other = Window(editor: DeckEditor(session: try ScaenaSession(directory: b1))).editor
    let theme = try other.session.themeText()?.theme
    let refusing = try #require(try await other.rethemeAway(.ships("Ember")), "the re-theme was answered")
    #expect(!refusing.applied && refusing.refused)
    #expect(try other.session.themeText()?.theme == theme, "a refused re-theme takes nothing")
}

/// The operation that sets the theme's paper to `hex`.
private enum AThemeEdit {
    static func paper(_ hex: String) -> [JSONValue] {
        [["op": "replace", "path": "/tokens/color/paper", "value": .string(hex)]]
    }
}

/// Whether a theme edit was made on the main thread, as the thread it ran on tells it.
private final class ThemedOn: @unchecked Sendable {
    private let lock = NSLock()
    private var was: Bool?

    func take(main: Bool) {
        lock.withLock { was = main }
    }

    var taken: Bool? { lock.withLock { was } }
}

/// The window's editor, on the main actor, as the window holds it.
@MainActor private final class Window {
    let editor: DeckEditor

    init(editor: DeckEditor) {
        self.editor = editor
    }
}

/// A theme's JSON is decoded once, as its text is read (#1262): the Theme panel reads it for each
/// row it draws, and a shipped theme, its 181 layouts among it, decoded again for each kept the
/// window from answering after every edit. Ten thousand reads of a shipped theme take no time
/// that decoding it each time would.
@Test func aThemesJSONIsDecodedOnceAsItsTextIsRead() throws {
    let session = try ScaenaSession.create(theme: "Dusk", title: "Theme reads")
    let text = try #require(try session.themeText())
    let layouts = try #require(text.json?["layouts"]?.object)
    #expect(layouts.count > 100, "a shipped theme carries its layout library: \(layouts.count)")
    let clock = ContinuousClock()
    let decodeOnce = clock.measure { _ = try? JSONDecoder().decode(JSONValue.self, from: Data(text.text.utf8)) }
    var read = 0
    let reads = clock.measure {
        for _ in 0..<10_000 where text.json?["tokens"] != nil { read += 1 }
    }
    #expect(read == 10_000)
    #expect(reads < decodeOnce * 50, "10,000 reads took \(reads), one decode \(decodeOnce)")
}
