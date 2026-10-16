import UIKit
import XCTest

/// The iPad app as someone opens it (PLAN 4.2): launched to the document browser, and B1 opened in
/// its window from the browser, held landscape as a deck is edited. Each keeps a screenshot of
/// what it shows.
final class OpeningTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// The app launched with no deck: the document browser, which makes and opens decks.
    @MainActor
    func testAppLaunchesToTheDocumentBrowser() {
        let app = launched()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30), "the app did not come to the foreground")
        settle()
        keep(app, as: "launched")
        XCTAssertEqual(app.state, .runningForeground, "the app did not stay up")
    }

    /// The launch screen's New Deck reads on its button at 4.5 to 1 or more, as WCAG 2's AA asks of
    /// words, measured on the screen's own pixels. The screen draws the button in the app's accent
    /// and its words lighter over it, not white, so the accent is a shade deep enough for them
    /// (apps/ipad/Assets.xcassets). A deck the app brought back open is closed first.
    @MainActor
    func testNewDeckReadsOnItsButton() throws {
        let app = launched()
        let canvas = app.descendants(matching: .any).matching(identifier: "canvas").firstMatch
        if canvas.waitForExistence(timeout: 10) {
            app.buttons.matching(identifier: "BackButton").firstMatch.tap()
            XCTAssertTrue(wait(10) { !canvas.exists }, "the deck brought back did not close: \(app.debugDescription)")
        }
        let words = app.staticTexts.matching(NSPredicate(format: "label == 'New Deck'")).firstMatch
        XCTAssertTrue(words.waitForExistence(timeout: 30), "the launch screen offers no New Deck: \(app.debugDescription)")
        settle()
        let read = try Reading(of: XCUIScreen.main.screenshot().image, at: words.frame, in: app.frame)
        XCTAssertGreaterThanOrEqual(read.ratio, 4.5, "New Deck reads at \(read)")
    }

    /// The launch screen offers a new deck, or one written from a prompt (PLAN G.6): New from a
    /// Prompt… shows the brief over the document browser, Generate held until the deck has
    /// something to be about, and Cancel puts it away with no deck made. A deck the app brought
    /// back open is closed first, by its way back to the launch screen.
    @MainActor
    func testNewFromAPromptShowsTheBriefOverTheBrowser() {
        let app = launched()
        let canvas = app.descendants(matching: .any).matching(identifier: "canvas").firstMatch
        if canvas.waitForExistence(timeout: 10) {
            app.buttons.matching(identifier: "BackButton").firstMatch.tap()
            XCTAssertTrue(wait(10) { !canvas.exists }, "the deck brought back did not close: \(app.debugDescription)")
        }
        // The buttons as the launch screen shows them, by their words: the screen also keeps a
        // bare copy of each, out of sight, which a tap passes through.
        let prompting = app.staticTexts.matching(NSPredicate(format: "label == 'New from a Prompt…'")).firstMatch
        XCTAssertTrue(
            prompting.waitForExistence(timeout: 30), "the launch screen offers no New from a Prompt…: \(app.debugDescription)")
        XCTAssertTrue(
            app.staticTexts.matching(NSPredicate(format: "label == 'New Deck'")).firstMatch.exists,
            "the launch screen offers no New Deck: \(app.debugDescription)")
        prompting.tap()
        let topic = app.descendants(matching: .any).matching(identifier: "prompt-topic").firstMatch
        XCTAssertTrue(topic.waitForExistence(timeout: 10), "New from a Prompt… shows no brief: \(app.debugDescription)")
        let generate = app.buttons["prompt-generate"]
        XCTAssertTrue(generate.exists, "the brief has no Generate: \(app.debugDescription)")
        XCTAssertFalse(generate.isEnabled, "Generate is offered with nothing to write about")
        settle()
        // Each of the brief's fields says what it asks for beside it, not only an example in it.
        let audience = app.staticTexts.matching(NSPredicate(format: "label == 'Audience'")).firstMatch
        XCTAssertTrue(audience.exists, "the audience field has no label: \(app.debugDescription)")
        keep(app, as: "prompt")
        // And in a person's words (PLAN 3.18): a length as a range is written, with an en dash; a
        // link as what it may be, with an example, not as a syntax. The link's row stands lower in
        // the brief: where it is not shown, the brief is scrolled up.
        let length = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'prompt-length'"))
            .firstMatch
        XCTAssertTrue(length.waitForExistence(timeout: 5), "the brief has no length: \(app.debugDescription)")
        let lengthSaid = "\(length.label) \(length.value as? String ?? "")"
        XCTAssertTrue(
            lengthSaid.contains("5\u{2013}10 slides"), "the length is not written as a range: \(lengthSaid)")
        let link = app.textFields.matching(NSPredicate(format: "identifier == 'prompt-link'")).firstMatch
        if !link.waitForExistence(timeout: 3) { app.swipeUp() }
        XCTAssertTrue(link.waitForExistence(timeout: 5), "the brief has no link field: \(app.debugDescription)")
        let hint = link.placeholderValue ?? ""
        XCTAssertTrue(
            hint.contains("sec:AAPL") && !hint.contains("TICKER") && !hint.contains("owner/repo"),
            "the link field reads as a syntax: \(hint)")
        app.buttons["prompt-cancel"].tap()
        XCTAssertTrue(wait(10) { !topic.exists }, "Cancel left the brief up: \(app.debugDescription)")
        XCTAssertTrue(prompting.waitForExistence(timeout: 10), "Cancel went elsewhere than the launch screen: \(app.debugDescription)")
        XCTAssertFalse(canvas.exists, "Cancel opened a deck: \(app.debugDescription)")
    }

    /// B1 in its window: its forty states down the side, the first shown, and Play.
    @MainActor
    func testB1OpensInItsWindow() throws {
        let app = launched()
        try openB1(in: app)
        XCTAssertTrue(app.buttons["Play"].exists, "B1's window has no Play: \(app.debugDescription)")
        settle()
        keep(app, as: "b1")
        keepElements(app, as: "b1-elements")
        // What VoiceOver reads is what the window shows (PLAN 3.18): the slide's layout says its
        // label once, then the layout; and a symbol drawn beside a choice is not read.
        let layout = app.buttons.matching(NSPredicate(format: "identifier == 'layout-choose'")).firstMatch
        XCTAssertTrue(
            layout.waitForExistence(timeout: 10), "the cover's inspector offers no layout: \(app.debugDescription)")
        XCTAssertLessThanOrEqual(
            layout.label.components(separatedBy: "Layout").count, 2, "the layout says its label twice: \(layout.label)")
        XCTAssertTrue(
            layout.label.contains("Title") || (layout.value as? String) == "Title",
            "the layout does not say the cover's: \(layout.label)")
        XCTAssertFalse(layout.images.firstMatch.exists, "the layout's chevron is read: \(layout.debugDescription)")
        let draft = app.buttons.matching(NSPredicate(format: "label == 'Draft Presenter Notes'")).firstMatch
        if draft.exists {
            XCTAssertFalse(
                draft.images.firstMatch.exists, "the sparkles of Draft Presenter Notes are read: \(draft.debugDescription)")
        }
    }

    /// A text inserted from the toolbar is typed in, its words selected, as a presentation app's
    /// new text box is (PLAN 3.21).
    @MainActor
    func testATextInsertedIsTypedIn() throws {
        let app = launched()
        try openB1(in: app)
        let text = app.buttons.matching(identifier: "insert-Text").firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 10), "no Text in the toolbar: \(app.debugDescription)")
        text.tap()
        let typing = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Editing'")).firstMatch
        XCTAssertTrue(typing.waitForExistence(timeout: 5), "the text inserted is not typed in: \(app.debugDescription)")
        settle()
        keep(app, as: "inserted")
    }

    /// Code pasted on the slide is a code text in its language, its chip saying which (PLAN 2.133).
    @MainActor
    func testCodePastedShowsItsLanguage() throws {
        let app = launched()
        try openB1(in: app)
        UIPasteboard.general.string = "fn main() {\n    let total = 42;\n    println!(\"{total}\");\n}\n"
        let canvas = app.descendants(matching: .any).matching(identifier: "canvas").firstMatch
        XCTAssertTrue(canvas.waitForExistence(timeout: 10), "no canvas: \(app.debugDescription)")
        // A long press on the title offers the edit menu, as TouchTests's does, and its Paste is the
        // canvas's: ⌘V from XCUITest's keyboard reached no paste on the simulator.
        let title = canvas.descendants(matching: .any).matching(NSPredicate(format: "label == 'Scaena'")).firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 10), "no title on the canvas: \(app.debugDescription)")
        title.press(forDuration: 1.2)
        let paste = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Paste'")).firstMatch
        guard paste.waitForExistence(timeout: 10) else {
            keep(app, as: "no-paste-offered")
            throw Unseen(description: "the long press offers no Paste: \(app.debugDescription)")
        }
        paste.tap()
        // The menu's Paste reads what the test put on the pasteboard, which iPadOS guards with its
        // Allow Paste prompt: the system's, not the app's, whose main thread waits on the answer, so
        // the app is asked nothing until it is answered.
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allow = springboard.buttons["Allow Paste"]
        if allow.waitForExistence(timeout: 10) { allow.tap() }
        let chip = app.descendants(matching: .any).matching(identifier: "code-chip").firstMatch
        XCTAssertTrue(chip.waitForExistence(timeout: 10), "the code pasted shows no chip: \(app.debugDescription)")
        XCTAssertEqual(chip.value as? String, "Rust", "the chip does not say Rust: \(chip.debugDescription)")
        settle()
        keep(app, as: "code-pasted")
    }
}

/// How words read on what they are drawn over, as the screen shows them (WCAG 2): the color most of
/// the middle of their frame holds, the color there that stands out most against it, and the ratio
/// of the two's luminances.
struct Reading: CustomStringConvertible {
    /// The two colors, sRGB, 0 to 255 a channel.
    let ground: [Double]
    let words: [Double]

    /// WCAG 2's contrast ratio of the two, 1 to 21.
    var ratio: Double { Self.contrast(ground, words) }

    var description: String {
        "\(String(format: "%.2f", ratio)) to 1, \(Self.hex(words)) on \(Self.hex(ground))"
    }

    /// What `screen` shows in the middle of `frame`, points in `space`, the app's frame: clear of a
    /// button's rounded ends. A screenshot keeps the display's own pixels, and beside them which way
    /// they turn to be seen as the iPad is held: each pixel is read where it is kept.
    @MainActor
    init(of screen: UIImage, at frame: CGRect, in space: CGRect) throws {
        guard let image = screen.cgImage, let srgb = CGColorSpace(name: CGColorSpace.sRGB) else {
            throw Unseen(description: "the screenshot has no pixels")
        }
        let (width, height) = (image.width, image.height)
        var turn = screen.imageOrientation
        // One that says nothing of its turn, kept upright while the iPad is held otherwise, turns as
        // the iPad is held: held landscape left, a quarter left, as CI's screenshots are kept.
        if turn == .up, (width > height) != (space.width > space.height) {
            turn = XCUIDevice.shared.orientation == .landscapeRight ? .right : .left
        }
        let sideways = turn == .left || turn == .right
        guard turn == .up || turn == .down || sideways else {
            throw Unseen(description: "the screenshot is mirrored (\(turn.rawValue))")
        }
        // The screen as it is seen, and where each pixel of it is kept.
        let (seenWidth, seenHeight) = sideways ? (height, width) : (width, height)
        func kept(_ x: Int, _ y: Int) -> (Int, Int) {
            switch turn {
            case .left: return (width - 1 - y, x)
            case .right: return (y, height - 1 - x)
            case .down: return (width - 1 - x, height - 1 - y)
            default: return (x, y)
            }
        }
        guard space.width > 0, space.height > 0,
            abs(Double(seenWidth) / Double(seenHeight) - Double(space.width / space.height)) < 0.01
        else {
            throw Unseen(
                description: "the screenshot is \(seenWidth) × \(seenHeight) px as seen, the app \(space.size)")
        }
        // Its pixels in sRGB, four bytes each, as kept.
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        let filled = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard
                let context = CGContext(
                    data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: srgb, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard filled else { throw Unseen(description: "the screenshot's pixels could not be read") }
        // The middle of the frame, in the screen's pixels as seen.
        let scale = Double(seenWidth) / Double(space.width)
        let middle = frame.insetBy(dx: frame.width * 0.2, dy: frame.height * 0.15)
        let left = max(0, Int(Double(middle.minX) * scale))
        let right = min(seenWidth, Int(Double(middle.maxX) * scale))
        let top = max(0, Int(Double(middle.minY) * scale))
        let bottom = min(seenHeight, Int(Double(middle.maxY) * scale))
        guard left < right, top < bottom else {
            throw Unseen(description: "the words' frame \(frame) is off the screen")
        }
        let (xs, ys) = (left..<right, top..<bottom)
        func pixel(_ x: Int, _ y: Int) -> [Double] {
            let (kx, ky) = kept(x, y)
            let i = (ky * width + kx) * 4
            return [Double(bytes[i]), Double(bytes[i + 1]), Double(bytes[i + 2])]
        }
        // The ground: the commonest color there, each channel to within 8, averaged.
        var counts: [Int: Int] = [:]
        var sums: [Int: [Double]] = [:]
        for y in ys {
            for x in xs {
                let p = pixel(x, y)
                let key = (Int(p[0]) >> 3) << 10 | (Int(p[1]) >> 3) << 5 | Int(p[2]) >> 3
                counts[key, default: 0] += 1
                sums[key] = zip(sums[key] ?? [0, 0, 0], p).map { $0 + $1 }
            }
        }
        guard let commonest = counts.max(by: { $0.value < $1.value }), let sum = sums[commonest.key] else {
            throw Unseen(description: "the words' frame \(frame) holds no pixels")
        }
        let ground = sum.map { $0 / Double(commonest.value) }
        // The words: what stands out most against the ground.
        var words = ground
        for y in ys {
            for x in xs {
                let p = pixel(x, y)
                if Self.contrast(ground, p) > Self.contrast(ground, words) { words = p }
            }
        }
        self.ground = ground
        self.words = words
    }

    static func contrast(_ a: [Double], _ b: [Double]) -> Double {
        let (la, lb) = (luminance(a), luminance(b))
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    /// WCAG 2's relative luminance of an sRGB color, 0 to 255 a channel.
    static func luminance(_ rgb: [Double]) -> Double {
        let linear = rgb.map { c -> Double in
            let s = c / 255
            return s <= 0.04045 ? s / 12.92 : pow((s + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    /// A color as #RRGGBB.
    static func hex(_ rgb: [Double]) -> String {
        "#" + rgb.map { String(format: "%02X", Int($0.rounded())) }.joined()
    }
}
