#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ScaenaKit
import SwiftUI

#if os(macOS)
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
    /// The show that plays, if one does: what a remote drives (PLAN 4.10).
    private(set) static weak var current: Showing?

    /// Present `editor`'s deck from `state`, its cue first.
    static func play(_ editor: DeckEditor, from state: String?) {
        end()
        let showing = Showing(editor: editor, from: state)
        showing.ended = { Presenting.end() }
        current = showing
        RemoteHost.shared.follow(showing)
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
        current = nil
        RemoteHost.shared.follow(nil)
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
#else
/// The deck presented on the iPad (PLAN 4.2, 4.8). With an external display or an AirPlay screen,
/// the stage fills it and the iPad shows the presenter's view; with neither, the stage fills the
/// iPad. A display that comes while the deck plays takes the stage, and one that goes gives it back.
/// Paced as the Mac's and the browser's player pace a deck: a tap or a swipe to the left goes on, a
/// swipe to the right goes back, and a pinch closed ends, as do a keyboard's keys.
@MainActor
enum Presenting {
    /// What the iPad shows while the deck plays: the stage, or the presenter's view.
    private static weak var shown: UIViewController?
    private static var showing: Showing?
    /// The stage on the display the iPad drives, while the deck plays there.
    private static var stage: UIWindow?
    private static var watching: [NSObjectProtocol] = []
    /// The show that plays, if one does: what a remote drives (PLAN 4.10).
    static var current: Showing? { showing }

    /// Present `editor`'s deck from `state`, its cue first.
    static func play(_ editor: DeckEditor, from state: String?) {
        end()
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.session.role == .windowApplication && $0.activationState == .foregroundActive }
        guard var top = scene?.keyWindow?.rootViewController else { return }
        while let over = top.presentedViewController { top = over }
        let showing = Showing(editor: editor, from: state)
        showing.ended = { Presenting.end() }
        self.showing = showing
        RemoteHost.shared.follow(showing)
        place()
        let host = UIHostingController(rootView: ShowHere(showing: showing))
        host.modalPresentationStyle = .fullScreen
        host.view.backgroundColor = .black
        top.present(host, animated: true)
        shown = host
        // A display that comes or goes while the deck plays: once its scene is in place or gone.
        watching = [UIScene.willConnectNotification, UIScene.didDisconnectNotification].map { name in
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { _ in
                Task { @MainActor in Presenting.place() }
            }
        }
    }

    /// End the show: the stage and the presenter's view go, and the display the iPad drives shows
    /// the iPad's screen again.
    static func end() {
        for token in watching { NotificationCenter.default.removeObserver(token) }
        watching = []
        leave()
        showing = nil
        RemoteHost.shared.follow(nil)
        shown?.dismiss(animated: true)
        shown = nil
    }

    /// The stage where it belongs: on the display the iPad drives, where there is one, the iPad
    /// showing the presenter's view; else on the iPad.
    static func place() {
        guard let showing else { return }
        let display = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first {
                $0.session.role == .windowExternalDisplayNonInteractive
                    && ($0.activationState == .foregroundActive || $0.activationState == .foregroundInactive)
            }
        if let display {
            if stage?.windowScene !== display {
                leave()
                let window = UIWindow(windowScene: display)
                let host = UIHostingController(rootView: StageView(showing: showing))
                host.view.backgroundColor = .black
                window.rootViewController = host
                window.backgroundColor = .black
                // Over anything else the display shows, and the iPad's screen no longer mirrored.
                window.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue + 1)
                window.isHidden = false
                stage = window
            }
        } else {
            leave()
        }
        showing.elsewhere = stage != nil
    }

    /// The stage taken off the display it filled.
    private static func leave() {
        stage?.isHidden = true
        stage?.rootViewController = nil
        stage = nil
    }
}

/// The scene the system makes for a display the iPad drives, an external display or an AirPlay
/// screen (PLAN 4.8, `UIWindowSceneSessionRoleExternalDisplayNonInteractive` in the app's
/// Info.plist). It shows the stage while a deck plays (`Presenting.place`); otherwise no window of
/// the app's, and the display shows the iPad's screen.
final class DisplaySceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        Task { @MainActor in Presenting.place() }
    }

    func sceneDidDisconnect(_ scene: UIScene) {
        Task { @MainActor in Presenting.place() }
    }
}

/// What the iPad shows while its deck plays (PLAN 4.8): the presenter's view while the stage is
/// on a display it drives, else the stage.
struct ShowHere: View {
    let showing: Showing

    var body: some View {
        if showing.elsewhere {
            PresenterView(showing: showing)
        } else {
            StageView(showing: showing)
        }
    }
}

extension View {
    /// A touch's pace (PLAN 4.8): a swipe to the left goes on and one to the right goes back, as a
    /// page turns, and a pinch closed ends the show.
    func turns(_ showing: Showing) -> some View {
        self
            .gesture(
                DragGesture(minimumDistance: 24)
                    .onEnded { drag in
                        let across = drag.translation.width
                        guard abs(across) > max(abs(drag.translation.height), 48) else { return }
                        if across < 0 { showing.on() } else { showing.back() }
                    }
            )
            .simultaneousGesture(
                MagnifyGesture()
                    .onEnded { pinch in
                        if pinch.magnification < 0.75 { showing.ended() }
                    }
            )
    }
}
#endif

/// A show as it goes (PLAN 3.5): where it is, as both windows draw it, and the time since it began.
@MainActor @Observable
final class Showing {
    let editor: DeckEditor
    private(set) var show: Show
    let began = Date()
    /// Ends the show: Escape's.
    @ObservationIgnored var ended: () -> Void = {}
    /// Whether the stage is on a display this one drives, so that this one shows the presenter's
    /// view (PLAN 4.8).
    var elsewhere = false
    /// How each state reads (§3.12), by the deck's revision: what the stage says to VoiceOver.
    @ObservationIgnored private var readings: [String: String] = [:]
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
            #if os(macOS)
            return NSWorkspace.shared.open(url)
            #else
            UIApplication.shared.open(url)
            return true
            #endif
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

    /// How the state shown reads at rest (§3.12), each node read in turn, as the player's live
    /// region says it: the stage's words for VoiceOver.
    var reading: String {
        guard let slot = show.slot else { return "" }
        let key = "\(editor.revision)\u{1f}\(slot.state)"
        if let read = readings[key] { return read }
        let parts = (try? editor.session.reads(state: slot.state)) ?? []
        let read = parts.map(\.text).filter { !$0.isEmpty }.joined(separator: ". ")
        readings[key] = read
        return read
    }

    /// Where the show is, for VoiceOver: "3 of 12".
    var progress: String { "\(show.index + 1) of \(show.slots.count)" }
}

#if os(macOS)
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
#endif

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
                    #if !os(macOS)
                    .turns(showing)
                    #endif
            }
        }
        // One element, read as the slide reads; VoiceOver's swipes up and down go on and back,
        // and its escape ends the show.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(showing.reading)
        .accessibilityValue(showing.progress)
        .accessibilityIdentifier("stage")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: showing.on()
            case .decrement: showing.back()
            @unknown default: break
            }
        }
        .accessibilityAction(.escape) { showing.ended() }
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .paces(showing)
        .onAppear { focused = true }
    }
}

/// The presenter's view: the state shown as the audience sees it, the next at rest, the state's
/// notes, where it is in the deck, and the time since the deck began. Side by side where it is
/// wide; where it is tall, as an iPad held upright, the next and the notes under the state shown.
/// A click or a tap on the state shown goes on, as one on the stage does; on the iPad, the stage's
/// swipes and pinch work on it too, and End Show ends the show (PLAN 4.8).
struct PresenterView: View {
    let showing: Showing
    @FocusState private var focused: Bool
    @State private var notes = ""
    @State private var next: CGImage?
    @Environment(\.displayScale) private var scale

    var body: some View {
        let show = showing.show
        let size = showing.size
        GeometryReader { geometry in
            let wide = geometry.size.width >= geometry.size.height
            let layout =
                wide
                ? AnyLayout(HStackLayout(alignment: .top, spacing: 24))
                : AnyLayout(VStackLayout(alignment: .leading, spacing: 24))
            layout {
                VStack(alignment: .leading, spacing: 12) {
                    #if !os(macOS)
                    Button(role: .cancel) {
                        showing.ended()
                    } label: {
                        Label("End Show", systemImage: "xmark")
                    }
                    .accessibilityIdentifier("end-show")
                    #endif
                    if let slot = show.slot {
                        // As the stage shows it, the stage's clock drawn here: one cue, not two.
                        ScaenaCanvas(
                            session: showing.editor.session, state: slot.state, revision: showing.editor.revision,
                            playhead: .constant(Playhead(ms: show.playhead.ms, playing: false))
                        )
                        .aspectRatio(size.width / max(size.height, 1), contentMode: .fit)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .overlay {
                            Color.clear
                                .contentShape(Rectangle())
                                .onTapGesture { showing.on() }
                                #if !os(macOS)
                                .turns(showing)
                                #endif
                        }
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(showing.reading)
                        .accessibilityValue(showing.progress)
                        .accessibilityIdentifier("presented")
                        .accessibilityAddTraits(.isButton)
                        .accessibilityAction { showing.on() }
                    }
                    HStack {
                        Text(showing.progress)
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
                .frame(width: wide ? 360 : nil)
            }
        }
        .padding(24)
        #if !os(macOS)
        .background(Color.black)
        .environment(\.colorScheme, .dark)
        #endif
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
