import CoreGraphics
import Foundation
import ScaenaKit
import Testing

#if os(macOS)
import AppKit
#endif

/// Code pasted, as the browser pastes it (PLAN 2.133): Rust copied from a page, its `<pre>` classed
/// `language-rust`, pastes as a code text in Rust, tidied; its box says its language, which its chip
/// shows; the inspector offers the languages as a person calls them; Copy Code gives its words
/// back; and Paste as Text, and prose, paste as words.
@Test func codePastesAsCodeInItsLanguage() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "goal"
    let session = editor.session
    let rust = "\tfn main() {\r\n\t    let total = 42;\r\n\t}\r\n"
    let hints = PasteHints(html: #"<pre class="language-rust"><code>fn main() {</code></pre>"#)
    let pasted = try session.pasting(rust, state: "goal", at: CGPoint(x: 960, y: 540), hints: hints)
    let node = try #require(pasted.patch.first?["node"])
    #expect(node["code"]?["language"]?.string == "rust")
    #expect(node["text"]?.string == "fn main() {\n    let total = 42;\n}")
    try editor.make(pasted.patch)

    let box = try #require(try session.boxes(state: "goal").first { $0.node == pasted.id })
    #expect(box.code == NodeBox.CodeLanguage(language: "rust", said: "Rust"))
    #expect(try session.codeWords(state: "goal", node: pasted.id) == "fn main() {\n    let total = 42;\n}")
    let language = try #require(try session.choices(state: "goal", node: pasted.id).fields.first { $0.prop == "code/language" })
    #expect(language.said["cpp"] == "C++" && language.value?.string == "rust")

    // Paste as Text, and prose, paste as words.
    let words = try session.pasting(rust, state: "goal", at: CGPoint(x: 100, y: 100), hints: PasteHints(plain: true))
    #expect(words.patch.first?["node"]?["code"] == nil)
    let prose = try session.pasting("Margins held through the quarter.", state: "goal", at: CGPoint(x: 100, y: 100))
    #expect(prose.patch.first?["node"]?["code"] == nil)
    // A file's name says its language.
    let named = try session.pasting("x = 1\nprint(x)\n", state: "goal", at: CGPoint(x: 100, y: 100), hints: PasteHints(file: "tally.py"))
    #expect(named.patch.first?["node"]?["code"]?["language"]?.string == "python")
    #expect(try session.tidied("\t\tx = 1\n\t\ty = 2\n", language: "python") == "x = 1\ny = 2")
}

#if os(macOS)
/// What the pasteboard says of the words it holds (PLAN 2.133): the HTML copied with them, and
/// Chromium's custom data, where VS Code says their language, in base64.
@MainActor
@Test func thePasteboardSaysWhatItsWordsAre() throws {
    let board = NSPasteboard.withUniqueName()
    defer { board.releaseGlobally() }
    board.clearContents()
    board.setString("fn main() {}", forType: .string)
    board.setString(#"<pre class="language-rust">"#, forType: .html)
    board.setData(Data([1, 2, 3, 4]), forType: NSPasteboard.PasteboardType("org.chromium.web-custom-data"))
    let hints = Pasteboard.hints(board)
    #expect(hints.html == #"<pre class="language-rust">"#)
    #expect(hints.chromium == Data([1, 2, 3, 4]).base64EncodedString())
    #expect(Pasteboard.words(board) == "fn main() {}")
    Pasteboard.write(words: "SELECT 1;", to: board)
    #expect(board.string(forType: .string) == "SELECT 1;" && Pasteboard.read(board) == .words("SELECT 1;"))
}

/// Words pasted into code being typed (PLAN 2.133): tidied as a paste of code is, then each line
/// after the first indented as the line it is pasted into is.
@MainActor
@Test func aPasteIntoCodeTakesItsLinesIndentation() throws {
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    editor.shown = "goal"
    let session = editor.session
    let block = "fn main() {\n    run();\n}"
    let pasted = try session.pasting(block, state: "goal", at: CGPoint(x: 960, y: 540), hints: PasteHints(file: "main.rs"))
    try editor.make(pasted.patch)
    let typing = Typing(editor: editor)
    #expect(typing.enter(pasted.id, in: "goal", at: nil))
    // The middle line's words, after its four spaces.
    typing.select(NSRange(location: 16, length: 6))
    typing.paste("\t\tlet a = 1;\n\t\tlet b = 2;\n")
    #expect(typing.carets?.text == "fn main() {\n    let a = 1;\n    let b = 2;\n}")
}
#endif
