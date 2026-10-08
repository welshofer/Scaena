import AppKit
import ScaenaKit
import SwiftUI

/// The deck presented (PLAN 3.5): the stage fills the external display where there is one, and
/// this one where there is not; with an external display, the presenter's window takes this one:
/// the state shown, the next, its notes, and the time since the deck began. It is paced as the
/// browser's player paces a deck (`Show`, PLAN 2.2): → ↓ Page Down Space Return and a click go on,
/// ← ↑ Page Up Delete go back, Home and End go to the first and the last state, and Escape ends.
/// A state that holds goes on by itself once its cue and its hold are over. A click on a link at
/// rest follows it (PLAN 2.70): a state is shown, a web address opens in the browser.
@MainActor
enum Presenting {
    private static var windows: [NSWindow] = []
    private static var closing: NSObjectProtocol?

    /// Present `editor`'s deck from `state`, its cue first.
    static func play(_ editor: DeckEditor, from state: String?) {
        end()
        let showing = Showing(editor: editor, from: state)
        showing.ended = { Presenting.end() }
        let main = NSScreen.main ?? NSScreen.screens.first
        let external = NSScreen.screens.first { $0 != main }
        guard let screen = external ?? main else { return }
        let stage = StageWindow(screen: screen, content: StageView(showing: showing))
        windows = [stage]
        if external != nil, let main {
            let presenter = NSWindow(
                contentRect: main.visibleFrame, styleMask: [.titled, .closable, .miniaturizable, .resizable],
                backing: .buffered, defer: false)
            presenter.title = "Presenter"
            presenter.isReleasedWhenClosed = false
            presenter.contentView = NSHostingView(rootView: PresenterView(showing: showing))
            presenter.setFrame(main.visibleFrame, display: true)
            // Closing the presenter's window ends the show.
            closing = NotificationCenter.default.addObserver(
                forName: NSWindow.willCloseNotification, object: presenter, queue: .main
            ) { _ in
                Task { @MainActor in Presenting.end() }
            }
            windows.append(presenter)
            stage.orderFrontRegardless()
            presenter.makeKeyAndOrderFront(nil)
        } else {
            stage.makeKeyAndOrderFront(nil)
        }
        NSApp.activate()
        NSCursor.setHiddenUntilMouseMoves(true)
    }

    /// End the show: its windows close.
    static func end() {
        if let closing { NotificationCenter.default.removeObserver(closing) }
        closing = nil
        let open = windows
        windows = []
        for window in open {
            window.orderOut(nil)
            window.close()
        }
    }
}

/// A show as it goes (PLAN 3.5): where it is, as both windows draw it, and the time since it began.
@MainActor @Observable
final class Showing {
    let editor: DeckEditor
    private(set) var show: Show
    let began = Date()
    /// Ends the show: Escape's.
    @ObservationIgnored var ended: () -> Void = {}
    /// The hold of a state come to rest, running until it gives way to the next.
    @ObservationIgnored private var holding: Task<Void, Never>?

    init(editor: DeckEditor, from state: String?) {
        self.editor = editor
        show = Show(slots: editor.slots, from: state)
    }

    /// Go on: a cue playing finishes, at rest the next state's plays.
    func on() {
        let finishing = show.playhead.playing
        holding?.cancel()
        show.on()
        if finishing { rested() }
    }

    func back() {
        holding?.cancel()
        show.back()
    }

    func first() {
        holding?.cancel()
        show.first()
    }

    func last() {
        holding?.cancel()
        show.last()
    }

    /// Where the stage's canvas says it is in the cue. A cue come to rest starts the state's hold.
    func told(_ playhead: Playhead) {
        let rests = show.playhead.playing && !playhead.playing
        show.playhead = playhead
        if rests { rested() }
    }

    /// The state shown at rest: where it holds, it goes on by itself once its hold is over.
    private func rested() {
        holding?.cancel()
        guard let hold = show.holds else { return }
        let index = show.index
        holding = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(hold.rounded())))
            guard !Task.isCancelled, let self, self.show.index == index, !self.show.playhead.playing else { return }
            self.show.on()
        }
    }

    /// A click at `location` on a stage `stage` points across: a link there at rest followed
    /// (PLAN 2.70), a state shown, a web address opened in the browser; else on, as any click.
    func click(at location: CGPoint, on stage: CGSize) {
        if let link = link(at: location, on: stage), follow(link) { return }
        on()
    }

    /// The link drawn at `location` on the stage, the state shown at rest: none while its cue
    /// plays, as in the browser's player.
    private func link(at location: CGPoint, on stage: CGSize) -> LinkTarget? {
        guard let slot = show.slot, !show.playhead.playing else { return nil }
        let size = self.size
        let scale = min(stage.width / max(size.width, 1), stage.height / max(size.height, 1))
        guard scale > 0 else { return nil }
        // The canvas fits the stage, in its middle.
        let x = (location.x - (stage.width - size.width * scale) / 2) / scale
        let y = (location.y - (stage.height - size.height * scale) / 2) / scale
        guard x >= 0, y >= 0, x <= size.width, y <= size.height else { return nil }
        return (try? editor.session.link(state: slot.state, at: CGPoint(x: x, y: y))) ?? nil
    }

    /// Follow `link`: whether it went anywhere.
    private func follow(_ link: LinkTarget) -> Bool {
        switch link {
        case .href(let href):
            guard let url = URL(string: href) else { return false }
            return NSWorkspace.shared.open(url)
        case .state(let state):
            holding?.cancel()
            return show.go(to: state)
        }
    }

    /// `state`'s notes, for the presenter.
    func notes(_ state: String) -> String {
        let choices = try? editor.session.stateChoices(state: state)
        return choices?.fields.first { $0.prop == "notes" }?.value?.string ?? ""
    }

    /// The canvas, canvas units.
    var size: CGSize { (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080) }
}

/// A borderless window filling a screen above its menu bar, that takes the keys.
final class StageWindow: NSWindow {
    convenience init(screen: NSScreen, content: some View) {
        self.init(contentRect: screen.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        isReleasedWhenClosed = false
        backgroundColor = .black
        level = NSWindow.Level(rawValue: NSWindow.Level.mainMenu.rawValue + 1)
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        contentView = NSHostingView(rootView: content)
        setFrame(screen.frame, display: true)
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

extension View {
    /// The player's keys (PLAN 2.2): on, back, the first and the last state, and Escape to end.
    func paces(_ showing: Showing) -> some View {
        self
            .onKeyPress(keys: [.rightArrow, .downArrow, .pageDown, .space, .return]) { _ in
                showing.on()
                return .handled
            }
            .onKeyPress(keys: [.leftArrow, .upArrow, .pageUp, .delete]) { _ in
                showing.back()
                return .handled
            }
            .onKeyPress(.home) {
                showing.first()
                return .handled
            }
            .onKeyPress(.end) {
                showing.last()
                return .handled
            }
            .onKeyPress(.escape) {
                showing.ended()
                return .handled
            }
    }
}

/// The stage: the state shown, its cue played, on black.
struct StageView: View {
    let showing: Showing
    @FocusState private var focused: Bool

    var body: some View {
        let size = showing.size
        ZStack {
            Color.black
            if let slot = showing.show.slot {
                ScaenaCanvas(
                    session: showing.editor.session, state: slot.state, revision: showing.editor.revision,
                    playhead: Binding(get: { showing.show.playhead }, set: { showing.told($0) })
                )
                .aspectRatio(size.width / max(size.height, 1), contentMode: .fit)
            }
        }
        .ignoresSafeArea()
        .overlay {
            GeometryReader { geometry in
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture(coordinateSpace: .local) { location in
                        showing.click(at: location, on: geometry.size)
                    }
            }
        }
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .paces(showing)
        .onAppear { focused = true }
    }
}

/// The presenter's window: the state shown as the audience sees it, the next at rest, the state's
/// notes, where it is in the deck, and the time since the deck began.
struct PresenterView: View {
    let showing: Showing
    @FocusState private var focused: Bool
    @State private var notes = ""
    @State private var next: CGImage?
    @Environment(\.displayScale) private var scale

    var body: some View {
        let show = showing.show
        let size = showing.size
        HStack(alignment: .top, spacing: 24) {
            VStack(alignment: .leading, spacing: 12) {
                if let slot = show.slot {
                    // As the stage shows it, the stage's clock drawn here: one cue, not two.
                    ScaenaCanvas(
                        session: showing.editor.session, state: slot.state, revision: showing.editor.revision,
                        playhead: .constant(Playhead(ms: show.playhead.ms, playing: false))
                    )
                    .aspectRatio(size.width / max(size.height, 1), contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                }
                HStack {
                    Text("\(show.index + 1) of \(show.slots.count)")
                    Spacer()
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(elapsed(at: context.date)).monospacedDigit()
                    }
                }
                .font(.title2)
            }
            .frame(maxWidth: .infinity)
            VStack(alignment: .leading, spacing: 10) {
                Text("Next").font(.headline).foregroundStyle(.secondary)
                if let next {
                    Image(decorative: next, scale: scale).resizable().aspectRatio(contentMode: .fit)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                } else {
                    Text(show.next == nil ? "The end" : "").foregroundStyle(.secondary)
                }
                Text("Notes").font(.headline).foregroundStyle(.secondary).padding(.top, 8)
                ScrollView {
                    Text(notes.isEmpty ? "No notes." : notes)
                        .font(.title3)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
            }
            .frame(width: 360)
        }
        .padding(24)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .paces(showing)
        .onAppear { focused = true }
        .task(id: show.index) {
            notes = show.slot.map { showing.notes($0.state) } ?? ""
            next = show.next.flatMap { showing.editor.drawing($0.state, width: Int(360 * scale)) }
        }
    }

    private func elapsed(at now: Date) -> String {
        let seconds = max(Int(now.timeIntervalSince(showing.began)), 0)
        return String(format: "%ld:%02ld", seconds / 60, seconds % 60)
    }
}
