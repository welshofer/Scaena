#if os(macOS)
import AppKit
#endif
import ScaenaKit
import SwiftUI

/// The wheel and a pinch over the canvas (PLAN 3.16), as the browser's preview takes them (PLAN
/// 2.46): a pinch, or the wheel with ⌘, zooms about the pointer; the wheel alone pans what is
/// zoomed in, and passes on where the whole canvas shows. It reads them as the window gets them,
/// before any view, and takes no press. The iPad's pinch and pan come with PLAN 4.3.
#if os(macOS)
struct CanvasWheel: NSViewRepresentable {
    /// Zoom by a factor about a point on the view.
    let zoom: (Double, CGPoint) -> Void
    /// Pan by a move on the view, points: whether it panned.
    let pan: (CGVector) -> Bool

    func makeNSView(context: Context) -> WheelView {
        let view = WheelView()
        updateNSView(view, context: context)
        return view
    }

    func updateNSView(_ view: WheelView, context: Context) {
        view.zoom = zoom
        view.pan = pan
    }

    final class WheelView: NSView {
        var zoom: ((Double, CGPoint) -> Void)?
        var pan: ((CGVector) -> Bool)?
        /// Removed as the view leaves its window, or goes.
        nonisolated(unsafe) private var monitor: Any?

        override var isFlipped: Bool { true }

        /// Presses go through to the canvas under it.
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel, .magnify]) { [weak self] event in
                guard let self else { return event }
                return self.take(event)
            }
        }

        /// `event`, over the canvas, taken: none where it zoomed or panned, else the event, for
        /// what is under the pointer.
        private func take(_ event: NSEvent) -> NSEvent? {
            guard let window, event.window === window else { return event }
            let at = convert(event.locationInWindow, from: nil)
            guard bounds.contains(at) else { return event }
            switch event.type {
            case .magnify:
                zoom?(1 + Double(event.magnification), at)
                return nil
            case .scrollWheel:
                // A wheel that goes by lines goes a line's worth of points at a time.
                let line = event.hasPreciseScrollingDeltas ? 1.0 : 16.0
                if event.modifierFlags.contains(.command) {
                    zoom?(exp(Double(event.scrollingDeltaY) * line / 240), at)
                    return nil
                }
                let by = CGVector(dx: -Double(event.scrollingDeltaX) * line, dy: -Double(event.scrollingDeltaY) * line)
                return pan?(by) == true ? nil : event
            default:
                return event
            }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }
    }
}
#endif

/// What the View menu does to the canvas in the window in front (PLAN 3.16): zoom a step closer or
/// farther, show the whole canvas, and find in the deck.
struct CanvasActions {
    /// A step closer (`1`) or farther (`-1`), about the middle of what is shown.
    let zoom: (Int) -> Void
    let fit: () -> Void
    /// Whether it is zoomed in.
    let zoomed: Bool
    /// Open the find bar.
    let find: () -> Void
}

private struct CanvasActionsKey: FocusedValueKey {
    typealias Value = CanvasActions
}

extension FocusedValues {
    /// What the View menu does to the canvas in the window in front.
    var canvasActions: CanvasActions? {
        get { self[CanvasActionsKey.self] }
        set { self[CanvasActionsKey.self] = newValue }
    }
}

/// The View menu's canvas commands (PLAN 3.16), as the browser's (PLAN 2.46, 2.47): Zoom In (⌘=),
/// Zoom Out (⌘−), the Whole Canvas (⌘0), and Find in Deck (⇧⌘F), which ⌘F on the canvas opens
/// too.
struct ViewCommands: Commands {
    @FocusedValue(\.canvasActions) private var canvas

    var body: some Commands {
        CommandGroup(after: .toolbar) {
            Divider()
            Button("Zoom In") { canvas?.zoom(1) }
                .keyboardShortcut("=", modifiers: .command)
                .disabled(canvas == nil)
            Button("Zoom Out") { canvas?.zoom(-1) }
                .keyboardShortcut("-", modifiers: .command)
                .disabled(canvas?.zoomed != true)
            Button("Whole Canvas") { canvas?.fit() }
                .keyboardShortcut("0", modifiers: .command)
                .disabled(canvas?.zoomed != true)
            Divider()
            Button("Find in Deck…") { canvas?.find() }
                .keyboardShortcut("f", modifiers: [.command, .shift])
                .disabled(canvas == nil)
        }
    }
}

/// The find bar over the canvas (PLAN 3.16, as the browser's, PLAN 2.47, 2.83): what is sought,
/// found in the deck's words in every state (its texts, each node's description, each state's
/// notes, and each beat's claim and notes), once for each place each is written. Return goes to
/// the next match, Shift+Return to the one before, each shown in its state with its node
/// selected; Replace replaces the match shown, Replace All every match in one patch, each one step
/// to undo. Escape closes it.
struct FindBar: View {
    let editor: DeckEditor
    /// Show a match: its state, and its node selected where its words are a node's.
    let reveal: (_ state: String?, _ node: String?) -> Void
    /// Make a replacement's patch: one step to undo.
    let make: ([JSONValue]) -> Void
    let close: () -> Void
    @State private var text = ""
    @State private var replacement = ""
    @State private var matchCase = false
    @State private var words = false
    @State private var found: [Found] = []
    /// The match shown, by its place among them all.
    @State private var now: Int?
    /// A match replaced: shown again once the deck is searched again.
    @State private var replaced = false
    @State private var said: String?
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            TextField("Find in the deck", text: $text)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 140, idealWidth: 200)
                .focused($focused)
                .onSubmit { go(Held.shift ? -1 : 1) }
            Toggle("Aa", isOn: $matchCase)
                .toggleStyle(.button)
                .help("Match case")
            Toggle("W", isOn: $words)
                .toggleStyle(.button)
                .help("Whole words only")
            Button {
                go(-1)
            } label: {
                Image(systemName: "chevron.up")
            }
            .help("The match before (⇧↩)")
            .disabled(matches.all.isEmpty)
            Button {
                go(1)
            } label: {
                Image(systemName: "chevron.down")
            }
            .help("The next match (↩)")
            .disabled(matches.all.isEmpty)
            Text(said ?? count)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(minWidth: 80, alignment: .leading)
            TextField("Replace with", text: $replacement)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 110, idealWidth: 160)
                .onSubmit { replaceOne() }
            Button("Replace") { replaceOne() }
                .help("Replace the match shown, then show the next")
                .disabled(matches.all.isEmpty)
            Button("Replace All") { replaceAll() }
                .help("Replace every match: one step to undo")
                .disabled(matches.all.isEmpty)
            Spacer(minLength: 0)
            Button {
                close()
            } label: {
                Image(systemName: "xmark.circle.fill")
            }
            .buttonStyle(.borderless)
            .help("Close the find bar (Escape)")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .onAppear { focused = true }
        #if os(macOS)
        .onExitCommand { close() }
        #else
        .onKeyPress(.escape) {
            close()
            return .handled
        }
        #endif
        .task(id: "\(text)\u{1f}\(matchCase)\u{1f}\(words)\u{1f}\(editor.revision)") { search() }
    }

    private var matches: Matches { Matches(found) }

    /// How many matches, and which is shown.
    private var count: String {
        if text.isEmpty { return "" }
        let n = matches.all.count
        if n == 0 { return "no matches" }
        if let now, now < n { return "\(now + 1) of \(n)" }
        return n == 1 ? "1 match" : "\(n) matches"
    }

    /// The deck searched again: as what is sought changed, or the deck did.
    private func search() {
        said = nil
        guard !text.isEmpty else {
            found = []
            now = nil
            return
        }
        do {
            found = try editor.session.find(text, matchCase: matchCase, words: words)
        } catch {
            found = []
            said = "\(error)"
        }
        let n = matches.all.count
        if let shown = now, shown >= n { now = n > 0 ? 0 : nil }
        if replaced {
            replaced = false
            if let now { show(now) }
        }
    }

    /// The next match (`1`), or the one before (`-1`), shown.
    private func go(_ by: Int) {
        let next = by > 0 ? matches.next(after: now) : matches.previous(before: now)
        guard let next else {
            said = text.isEmpty ? nil : "no matches"
            return
        }
        now = next
        show(next)
    }

    /// Match `n` shown: its state, and its node selected.
    private func show(_ n: Int) {
        guard matches.all.indices.contains(n) else { return }
        let f = found[matches.all[n].text]
        reveal(f.state, f.kind == "text" || f.kind == "alt" ? f.node : nil)
    }

    /// The match shown replaced, one step to undo, then the next shown.
    private func replaceOne() {
        guard let shown = now, matches.all.indices.contains(shown) else { return go(1) }
        let m = matches.all[shown]
        do {
            let patch = try editor.session.replacing(
                text, with: replacement, matchCase: matchCase, words: words, one: (text: m.text, match: m.match))
            make(patch)
            // What it put in, where it holds what is sought again, is gone past.
            let again =
                matchCase ? replacement.contains(text) : replacement.localizedCaseInsensitiveContains(text)
            if again { now = shown + 1 }
            replaced = true
        } catch {
            said = "not replaced: \(error)"
        }
    }

    /// Every match replaced, in one patch: one step to undo.
    private func replaceAll() {
        do {
            let patch = try editor.session.replacing(text, with: replacement, matchCase: matchCase, words: words)
            guard !patch.isEmpty else {
                said = "nothing to replace"
                return
            }
            let (n, places) = (matches.all.count, found.count)
            make(patch)
            now = nil
            said = "\(n) replaced in \(places) \(places == 1 ? "place" : "places")"
        } catch {
            said = "not replaced: \(error)"
        }
    }
}

/// The state shown in each of the deck's formats side by side under the canvas (PLAN 3.16, as the
/// browser's, PLAN 2.62): its own canvas, then each format the deck lists, all one height, each
/// drawn at rest by the CPU painter, each format laying the state out once, and each counting the
/// findings about the state that hold there, in the color of the worst. A click shows the canvas in
/// that format.
struct FormatsStrip: View {
    let editor: DeckEditor
    let state: String
    /// Show the canvas in a format: none for the deck's own canvas.
    let open: (String?) -> Void
    @State private var figures: [Figure] = []

    /// One format drawn: `""` for the deck's own canvas.
    private struct Figure: Identifiable {
        let name: String
        let image: CGImage?
        var id: String { name }
    }

    /// How high each figure is, points.
    private static let height = 96.0

    var body: some View {
        ScrollView(.horizontal) {
            HStack(alignment: .top, spacing: 12) {
                ForEach(figures) { figure in
                    Button {
                        open(figure.name.isEmpty ? nil : figure.name)
                    } label: {
                        drawn(figure)
                    }
                    .buttonStyle(.plain)
                    .help("Edit the deck \(figure.name.isEmpty ? "on its own canvas" : "in \(figure.name)")")
                }
            }
            .padding(8)
        }
        .frame(height: Self.height + 40)
        .task(id: "\(state)\u{1f}\(editor.revision)") { draw() }
    }

    private func drawn(_ figure: Figure) -> some View {
        let pressed = figure.name == (editor.format ?? "")
        let found = editor.findings.filter { $0.state == state && ($0.formats ?? []).contains(figure.name) }
        return VStack(spacing: 4) {
            Group {
                if let image = figure.image {
                    Image(decorative: image, scale: 2)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                } else {
                    Rectangle().fill(.quaternary).frame(width: Self.height)
                }
            }
            .frame(height: Self.height)
            .clipShape(RoundedRectangle(cornerRadius: 3))
            .overlay(
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(pressed ? Color.accentColor : Color.secondary.opacity(0.3), lineWidth: pressed ? 2 : 1))
            HStack(spacing: 4) {
                Text(figure.name.isEmpty ? "own canvas" : figure.name).font(.caption)
                if let worst = found.map(\.severity).max() {
                    Text("\(found.count)")
                        .font(.caption2.monospacedDigit())
                        .padding(.horizontal, 5)
                        .background(Self.color(worst).opacity(0.25), in: Capsule())
                        .help("\(found.count) \(found.count == 1 ? "issue" : "issues") on this slide in this size")
                }
            }
        }
    }

    /// Each figure drawn again: as the state shown, the deck, or its formats changed.
    private func draw() {
        let session = editor.session
        let names = [""] + ((try? session.formats()) ?? [])
        figures = names.map { name in
            let pixels = try? session.pixels(in: name.isEmpty ? nil : name, state: state, height: Int(Self.height * 2))
            return Figure(name: name, image: pixels?.image)
        }
    }

    private static func color(_ severity: Severity) -> Color {
        switch severity {
        case .error: .red
        case .warning: .orange
        case .info: .blue
        }
    }
}
