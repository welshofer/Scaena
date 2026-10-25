#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ScaenaKit
import ScaenaNotes
import SwiftUI

#if os(macOS)
/// The deck presented (PLAN 3.5): the stage fills the external display where there is one, and
/// this one where there is not; with an external display, the presenter's window takes this one:
/// the state shown, the next, its notes, a timer that pauses and resets, and the time of day
/// (PLAN 2.121). It is paced as the browser's player paces a deck (`Show`, PLAN 2.2): → ↓ Page
/// Down Space Return and a click go on, ← ↑ Page Up Delete go back, Home and End go to the first
/// and the last state, and Escape ends. Going on past the last state, a black screen says the show
/// has ended, and going on again ends it, as Keynote's and PowerPoint's do (PLAN 2.120).
/// A state that holds goes on by itself once its cue and its hold are over. A click on a link at
/// rest follows it (PLAN 2.70): a state is shown, a web address opens in the browser. The
/// presenter's keys are the browser's player's (PLAN 2.105): B or . a black screen and W or , a
/// white one, until a key or a click brings the slide back; a slide's number then Return; L the
/// laser, where the pointer is over the slide or the presenter's; and ? the keys, listed. While
/// the deck plays, the screen stays awake (`Awake`), and a display that comes or goes moves the
/// stage: a projector plugged in after Play takes it, and one unplugged gives it back (`Staging`,
/// lift-up REL-1).
@MainActor
enum Presenting {
    private static var showing: Showing?
    /// The stage, while the deck plays.
    private static var stage: StageWindow?
    /// The presenter's window, while the stage is on another display.
    private static var presenter: NSWindow?
    private static var closing: NSObjectProtocol?
    /// The displays watched for one that comes or goes while the deck plays (lift-up REL-1).
    private static var displays: NSObjectProtocol?
    /// The display the deck is edited on, where the presenter's window goes: the key window's as
    /// the show began, whatever window is key after.
    private static var home: CGDirectDisplayID?
    /// The screen kept awake while the deck plays (lift-up REL-1).
    private static var awake: Awake?
    /// The show that plays, if one does: what a remote drives (PLAN 4.10).
    static var current: Showing? { showing }
    /// What is told, as the show ends, the state it stopped on.
    private static var stopped: ((String) -> Void)?

    /// Present `editor`'s deck from `state`, its cue first; ended, `stopped` is told the state it
    /// stopped on, which the window then shows, as a presentation app leaves you where you stopped.
    static func play(_ editor: DeckEditor, from state: String?, stopped: ((String) -> Void)? = nil) {
        end()
        Self.stopped = stopped
        let showing = Showing(editor: editor, from: state)
        showing.ended = { Presenting.end() }
        self.showing = showing
        RemoteHost.shared.follow(showing)
        guard let first = NSScreen.main ?? NSScreen.screens.first else { return }
        home = first.displayID
        awake = Awake(reason: "Scaena is presenting a deck")
        stage = StageWindow(screen: first, content: StageView(showing: showing))
        place()
        // A display that comes or goes while the deck plays: a projector plugged in after Play
        // takes the stage, and one unplugged gives it back (lift-up REL-1).
        displays = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { _ in
            Task { @MainActor in Presenting.place() }
        }
        NSApp.activate()
        NSCursor.setHiddenUntilMouseMoves(true)
    }

    /// The stage where it belongs (`Staging`): on a display other than the one the deck is edited
    /// on where there is one, the presenter's window on that one; else on it alone. Read again as
    /// displays come and go, each window moved where it now belongs, the presenter's opened or closed.
    static func place() {
        guard let showing, let stage else { return }
        let screens = NSScreen.screens
        guard let placed = Staging(home: home, among: screens.map(\.displayID)) else { return }
        let onto = screens.first { $0.displayID == placed.stage } ?? screens[0]
        if stage.frame != onto.frame { stage.setFrame(onto.frame, display: true) }
        showing.elsewhere = placed.presenter != nil
        let main = placed.presenter.flatMap { id in screens.first { $0.displayID == id } }
        switch (main, presenter) {
        case (let main?, nil):
            let window = NSWindow(
                contentRect: main.visibleFrame, styleMask: [.titled, .closable, .miniaturizable, .resizable],
                backing: .buffered, defer: false)
            window.title = "Presenter"
            window.isReleasedWhenClosed = false
            window.contentView = NSHostingView(rootView: PresenterView(showing: showing))
            window.setFrame(main.visibleFrame, display: true)
            // Closing the presenter's window ends the show.
            closing = NotificationCenter.default.addObserver(
                forName: NSWindow.willCloseNotification, object: window, queue: .main
            ) { _ in
                Task { @MainActor in Presenting.end() }
            }
            presenter = window
            stage.orderFrontRegardless()
            window.makeKeyAndOrderFront(nil)
        case (let main?, let window?):
            if window.frame != main.visibleFrame { window.setFrame(main.visibleFrame, display: true) }
        case (nil, let window?):
            // The display the stage was on is gone: the stage comes home, and the presenter's
            // window goes, its close no longer the show's end.
            dismiss(window)
            stage.makeKeyAndOrderFront(nil)
        case (nil, nil):
            if !stage.isKeyWindow { stage.makeKeyAndOrderFront(nil) }
        }
    }

    /// The presenter's window closed by the show, not by a person: closing it ends nothing.
    private static func dismiss(_ window: NSWindow) {
        if let closing { NotificationCenter.default.removeObserver(closing) }
        closing = nil
        presenter = nil
        window.orderOut(nil)
        window.close()
    }

    /// End the show: its windows close, the screen may sleep, and the window that played it shows
    /// where it stopped.
    static func end() {
        let reached = showing?.show.slot?.state
        let tell = stopped
        stopped = nil
        defer { if let reached { tell?(reached) } }
        showing?.point(nil)
        Pointer.hidden(false)
        showing = nil
        RemoteHost.shared.follow(nil)
        if let displays { NotificationCenter.default.removeObserver(displays) }
        displays = nil
        awake?.end()
        awake = nil
        if let presenter { dismiss(presenter) }
        if let stage {
            stage.orderOut(nil)
            stage.close()
        }
        stage = nil
        home = nil
    }
}

extension NSScreen {
    /// The display this screen is, as the system numbers it: the same display across the screens
    /// AppKit makes for it as displays come and go.
    var displayID: CGDirectDisplayID {
        (deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value ?? 0
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
    /// The iPad kept from locking while the deck plays (lift-up REL-1): never touched while the
    /// remote drives it, it would.
    private static var awake: Awake?
    /// The show that plays, if one does: what a remote drives (PLAN 4.10).
    static var current: Showing? { showing }
    /// What is told, as the show ends, the state it stopped on.
    private static var stopped: ((String) -> Void)?

    /// Present `editor`'s deck from `state`, its cue first; ended, `stopped` is told the state it
    /// stopped on, which the window then shows, as a presentation app leaves you where you stopped.
    static func play(_ editor: DeckEditor, from state: String?, stopped: ((String) -> Void)? = nil) {
        end()
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.session.role == .windowApplication && $0.activationState == .foregroundActive }
        guard var top = scene?.keyWindow?.rootViewController else { return }
        while let over = top.presentedViewController { top = over }
        let showing = Showing(editor: editor, from: state)
        showing.ended = { Presenting.end() }
        self.showing = showing
        Self.stopped = stopped
        RemoteHost.shared.follow(showing)
        awake = Awake(reason: "Scaena is presenting a deck")
        place()
        // The status bar and the home indicator out of the slide's way while it plays, as a
        // presentation app's slideshow hides them.
        let host = StageHost(rootView: ShowHere(showing: showing).statusBarHidden(true).persistentSystemOverlays(.hidden))
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

    /// End the show: the stage and the presenter's view go, the display the iPad drives shows the
    /// iPad's screen again, the iPad may lock, and the window that played it shows where it stopped.
    static func end() {
        let reached = showing?.show.slot?.state
        let tell = stopped
        stopped = nil
        defer { if let reached { tell?(reached) } }
        for token in watching { NotificationCenter.default.removeObserver(token) }
        watching = []
        awake?.end()
        awake = nil
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

/// What presents the deck on the iPad (PLAN 4.8): the status bar and the home indicator hidden
/// while it plays, as a presentation app's slideshow hides them.
final class StageHost<Content: View>: UIHostingController<Content> {
    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }
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
    /// page turns, and a pinch closed ends the show. A touch held is the laser (PLAN 2.105), as
    /// Keynote's: the dot under the finger while it stays down, `slide` saying where on the slide
    /// a point of this view is; it turns no slide.
    func turns(_ showing: Showing, slide: @escaping (CGPoint) -> CGPoint?) -> some View {
        modifier(Turns(showing: showing, slide: slide))
    }
}

/// What `turns` does. A touch held whose gesture is cancelled, which calls no `onEnded`, is let go
/// all the same: SwiftUI puts `touching` back when the system takes the touch, and the view going,
/// as when a display comes and the presenter's view takes the stage's place, ends it too. Else the
/// dot would stay, and no tap turn a slide.
private struct Turns: ViewModifier {
    let showing: Showing
    let slide: (CGPoint) -> CGPoint?
    @GestureState private var touching = false

    func body(content: Content) -> some View {
        content
            // Beside the tap, which still turns a slide: a hold ended turns none (`lifting`).
            .simultaneousGesture(
                LongPressGesture(minimumDuration: 0.3)
                    .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .local))
                    .updating($touching) { held, touching, _ in
                        if case .second(true, _) = held { touching = true }
                    }
                    .onChanged { held in
                        if case .second(true, let drag?) = held { showing.hold(slide(drag.location)) }
                    }
                    .onEnded { _ in showing.hold(nil, ended: true) }
            )
            .onChange(of: touching) { _, now in
                if !now { showing.cancelHold() }
            }
            .onDisappear { showing.cancelHold() }
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
    /// The presenter's timer (PLAN 2.121): the time since the show began, paused and reset there.
    var stopwatch = Stopwatch()
    /// Ends the show: Escape's.
    @ObservationIgnored var ended: () -> Void = {}
    /// Whether the stage is on a display this one drives, so that this one shows the presenter's
    /// view (PLAN 4.8).
    var elsewhere = false
    /// How each state reads (§3.12), by the deck's revision: what the stage says to VoiceOver.
    @ObservationIgnored private var readings: [String: String] = [:]
    /// The hold of a state come to rest, running until it gives way to the next.
    @ObservationIgnored private var holding: Task<Void, Never>?
    /// The keys listed (PLAN 2.105), over the presenter's view, or the stage where there is none.
    var listing = false
    /// Whether the laser is on (PLAN 2.105), and where it points, as fractions of the slide, while
    /// the pointer is over it or a touch is held on it.
    var laser = false
    var pointer: CGPoint?
    /// The pointer's rest over the slide, after which it hides until it moves.
    @ObservationIgnored private var resting: Task<Void, Never>?
    /// What the keys say for a moment where a slide's number is typed: one the deck lacks.
    var said: String?
    @ObservationIgnored private var saying: Task<Void, Never>?
    /// The number typed let go after four seconds with no key, as the browser's.
    @ObservationIgnored private var typing: Task<Void, Never>?

    init(editor: DeckEditor, from state: String?) {
        self.editor = editor
        show = Show(slots: editor.slots, from: state)
    }

    /// Go on: a cue playing finishes, at rest the next state's plays; past the last, the show is
    /// over, and on from there ends it (PLAN 2.120).
    func on() {
        if uncovered() { return }
        if show.over { return ended() }
        let finishing = show.playhead.playing
        holding?.cancel()
        show.on()
        if finishing { rested() }
    }

    func back() {
        if uncovered() { return }
        holding?.cancel()
        show.back()
    }

    func first() {
        if uncovered() { return }
        holding?.cancel()
        show.first()
    }

    func last() {
        if uncovered() { return }
        holding?.cancel()
        show.last()
    }

    /// A key or a click while the screen is black or white brings the slide back, and does no
    /// more (PLAN 2.105): whether it did.
    private func uncovered() -> Bool {
        guard show.blank != nil else { return false }
        blanking(nil)
        return true
    }

    /// The screen `to`, black or white, the show waiting where it was; the slide again for none, or
    /// where the screen is that already. No hold runs while the slide is out of sight: the slide
    /// back, one the screen stopped runs on with what was left of it, as the browser's player's
    /// does, and none starts that was not running.
    func blanking(_ to: Blank?) {
        holding?.cancel()
        if let to { show.blanking(to) } else { show.unblank() }
        if let left = show.holding() { waits(left) }
    }

    /// `digit` typed on a slide's number, let go after four seconds with no other.
    func type(_ digit: Int) {
        said = nil
        show.type(digit)
        typing?.cancel()
        typing = Task { [weak self] in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled else { return }
            self?.show.untype(all: true)
        }
    }

    /// Return: the slide whose number is typed, said where the deck has none, and the show going on
    /// as it was, a hold that runs still running; else on.
    func enter() {
        guard !show.typed.isEmpty else { return on() }
        typing?.cancel()
        let number = show.typed
        if show.goTyped() {
            holding?.cancel()
        } else {
            let slides = show.slides == 1 ? "1 slide" : "\(show.slides) slides"
            say("No slide \(number): the deck has \(slides)")
        }
    }

    /// The slide `state` begins, chosen in the presenter's navigator (lift-up 10-24b USE-1): shown
    /// at once, as its number typed then Return shows it, a hold that ran let go.
    func navigate(to state: String) {
        typing?.cancel()
        if show.go(toSlide: state) { holding?.cancel() }
    }

    /// The slide numbered `number`, as typing its number then Return goes there (PLAN 2.105): what
    /// a remote's Go to Slide asks (lift-up FUNC-5).
    func go(slide number: Int) {
        guard number > 0 else { return }
        show.untype(all: true)
        for digit in String(number) { show.type(digit.wholeNumberValue ?? 0) }
        enter()
    }

    /// Delete: the last digit typed taken back; with none typed, back.
    func delete() {
        if show.typed.isEmpty { back() } else { show.untype() }
    }

    /// Escape: the number typed let go, the keys put away, or the show ended.
    func escape() {
        if !show.typed.isEmpty {
            show.untype(all: true)
        } else if listing {
            listing = false
        } else {
            ended()
        }
    }

    /// `words` where the number is typed, for three seconds.
    private func say(_ words: String) {
        said = words
        saying?.cancel()
        saying = Task { [weak self] in
            try? await Task.sleep(for: .seconds(3))
            guard !Task.isCancelled else { return }
            self?.said = nil
        }
    }

    /// The laser on, or off.
    func lasering() {
        laser.toggle()
        point(nil)
    }

    /// The pointer over the slide at `at`, fractions of it, or off it: the laser there while it is on.
    func point(_ at: CGPoint?) {
        pointer = laser ? at : nil
        #if os(macOS)
        Pointer.hidden(pointer != nil)
        resting?.cancel()
        // Over the slide, the pointer hides once it has rested two seconds, as a presentation
        // app's does, until it moves again (lift-up ATTR-4).
        guard at != nil, pointer == nil else { return }
        resting = Task {
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled else { return }
            NSCursor.setHiddenUntilMouseMoves(true)
        }
        #endif
    }

    /// A touch held on the slide at `at`, or off it, or let go (`ended`): the laser while it stays
    /// down, on or off.
    func hold(_ at: CGPoint?, ended: Bool = false) {
        if ended { touch.lift() } else { touch.hold(at) }
        pointer = touch.at
    }

    /// A touch held as the laser whose gesture ended without its lifting: cancelled, as when its
    /// view goes or the system takes the touch. The dot goes, and a tap turns a slide again.
    func cancelHold() {
        guard touch.down else { return }
        touch.cancel()
        pointer = nil
    }

    /// A touch held as the laser, while it is down, and when it was lifted.
    @ObservationIgnored private var touch = HeldTouch()

    /// Whether a tap now is the lifting of a touch held as the laser, which turns no slide.
    var lifting: Bool { touch.lifting() }

    /// Where `location` on a view `size` big that fits the canvas in its middle is on the slide, as
    /// fractions of it; none off it.
    func onSlide(_ location: CGPoint, in size: CGSize) -> CGPoint? {
        let canvas = self.size
        let scale = min(size.width / max(canvas.width, 1), size.height / max(canvas.height, 1))
        guard scale > 0 else { return nil }
        let w = canvas.width * scale
        let h = canvas.height * scale
        let x = (location.x - (size.width - w) / 2) / w
        let y = (location.y - (size.height - h) / 2) / h
        return (0...1).contains(x) && (0...1).contains(y) ? CGPoint(x: x, y: y) : nil
    }

    /// Where `point` on the slide, fractions of it, stands on a view `size` big that fits the
    /// canvas in its middle.
    func onStage(_ point: CGPoint, in size: CGSize) -> CGPoint {
        let canvas = self.size
        let scale = min(size.width / max(canvas.width, 1), size.height / max(canvas.height, 1))
        let w = canvas.width * scale
        let h = canvas.height * scale
        return CGPoint(x: (size.width - w) / 2 + point.x * w, y: (size.height - h) / 2 + point.y * h)
    }

    /// What the keys say where a slide's number is typed: the number, or a slide the deck lacks.
    var typedWords: String? {
        show.typed.isEmpty ? said : "Slide \(show.typed) · Return goes there"
    }

    /// Where the stage's canvas says it is in the cue. A cue come to rest starts the state's hold.
    func told(_ playhead: Playhead) {
        let rests = show.playhead.playing && !playhead.playing
        show.playhead = playhead
        if rests { rested() }
    }

    /// The state shown at rest: where it holds, it goes on by itself once its hold is over. The
    /// next state's cue is laid out meanwhile, so going on moves at once (lift-up PERF-1).
    private func rested() {
        holding?.cancel()
        if let next = show.next, next.span > 0 { try? editor.session.prepare(next.state) }
        guard let hold = show.rested() else { return }
        waits(hold)
    }

    /// The state shown gives way to the next by itself once `hold`, ms, is over, where it is still
    /// shown, at rest.
    private func waits(_ hold: Double) {
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
        if lifting { return }
        if uncovered() { return }
        if let link = link(at: location, on: stage), follow(link) { return }
        on()
    }

    /// The link drawn at `location` on the stage, the state shown at rest: none while its cue
    /// plays, as in the browser's player, nor once the show is over.
    private func link(at location: CGPoint, on stage: CGSize) -> LinkTarget? {
        guard let slot = show.slot, !show.playhead.playing, !show.over else { return nil }
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
            // Only a web address, as the engine draws one (`scaena_core::links`, lift-up SEC-2):
            // never a file, a share, or another app's scheme, one click into a show.
            guard let url = URL(string: href), ["https", "http", "mailto"].contains(url.scheme ?? "") else {
                return false
            }
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

    /// `state`'s notes, for the presenter and the remote (SPEC §2.6): its own, else its beat's,
    /// which a generated deck writes them on, as written and as the blocks a speaker reads.
    func notes(_ state: String) -> Notes {
        (try? editor.session.notes(state: state)) ?? .none
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

    /// Where the show is, for VoiceOver: "3 of 12", "3 of 12, black screen" while it is, and
    /// "12 of 12, end of the show" once it is over.
    var progress: String {
        let at = "\(show.index + 1) of \(show.slots.count)"
        if show.over { return "\(at), end of the show" }
        return show.blank.map { "\(at), \($0.rawValue) screen" } ?? at
    }
}

#if os(macOS)
/// The pointer hidden while the laser stands in for it over the slide (PLAN 2.105), and shown again
/// as it leaves: each hide met by one show.
@MainActor
enum Pointer {
    private static var hiding = false

    static func hidden(_ hide: Bool) {
        guard hide != hiding else { return }
        hiding = hide
        if hide { NSCursor.hide() } else { NSCursor.unhide() }
    }
}
#endif

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
    /// The player's keys (PLAN 2.2): on, back, the first and the last state, and Escape to end; and
    /// the presenter's (PLAN 2.105), as the browser's player's: B or . a black screen, W or , a
    /// white one, a slide's number then Return, L the laser, and ? the keys. Delete takes a digit
    /// typed back, and Escape lets the number go or puts the keys away before it ends the show.
    func paces(_ showing: Showing) -> some View {
        self
            .onKeyPress(keys: [.rightArrow, .downArrow, .pageDown, .space]) { _ in
                showing.on()
                return .handled
            }
            .onKeyPress(.return) {
                showing.enter()
                return .handled
            }
            .onKeyPress(keys: [.leftArrow, .upArrow, .pageUp]) { _ in
                showing.back()
                return .handled
            }
            .onKeyPress(.delete) {
                showing.delete()
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
                showing.escape()
                return .handled
            }
            .onKeyPress(characters: CharacterSet(charactersIn: "bB.")) { press in
                guard plain(press) else { return .ignored }
                showing.blanking(.black)
                return .handled
            }
            .onKeyPress(characters: CharacterSet(charactersIn: "wW,")) { press in
                guard plain(press) else { return .ignored }
                showing.blanking(.white)
                return .handled
            }
            .onKeyPress(characters: CharacterSet(charactersIn: "lL")) { press in
                guard plain(press) else { return .ignored }
                showing.lasering()
                return .handled
            }
            .onKeyPress(characters: CharacterSet(charactersIn: "?")) { press in
                guard plain(press) else { return .ignored }
                showing.listing.toggle()
                return .handled
            }
            .onKeyPress(characters: .decimalDigits) { press in
                guard plain(press), let digit = press.characters.first?.wholeNumberValue else { return .ignored }
                showing.type(digit)
                return .handled
            }
    }
}

/// A key pressed alone, or with Shift or Caps Lock: a menu's command keys stay the menu's.
private func plain(_ press: KeyPress) -> Bool {
    press.modifiers.subtracting([.shift, .capsLock, .numericPad]).isEmpty
}

/// The stage: the state shown, its cue played, on black; black or white while the presenter has
/// it so, and the laser's dot where it points (PLAN 2.105); and black, saying so, once the show is
/// over (PLAN 2.120).
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
            // The screen black or white: the slide out of sight, the show waiting where it was.
            if let blank = showing.show.blank {
                blank == .black ? Color.black : Color.white
            }
            // Past the last slide: the show has ended, and says so.
            if showing.show.over { ShowOver() }
        }
        .ignoresSafeArea()
        .overlay {
            GeometryReader { geometry in
                ZStack {
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture(coordinateSpace: .local) { location in
                            showing.click(at: location, on: geometry.size)
                        }
                        // The pointer over the slide points the laser, while it is on.
                        .onContinuousHover(coordinateSpace: .local) { phase in
                            switch phase {
                            case .active(let at): showing.point(showing.onSlide(at, in: geometry.size))
                            case .ended: showing.point(nil)
                            }
                        }
                        #if !os(macOS)
                        .turns(showing) { showing.onSlide($0, in: geometry.size) }
                        #endif
                    if let at = showing.pointer, showing.show.blank == nil, !showing.show.over {
                        Laser(across: min(geometry.size.width, geometry.size.height))
                            .position(showing.onStage(at, in: geometry.size))
                    }
                }
            }
        }
        // One element, read as the slide reads, or as the end of the show once it is over;
        // VoiceOver's swipes up and down go on and back, and its escape ends the show.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(showing.show.over ? ShowOver.ended : showing.reading)
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
        // What the presenter's keys say, where no presenter's view says it: a slide's number typed,
        // and the keys when ? lists them.
        .overlay(alignment: .bottom) {
            if !showing.elsewhere, let words = showing.typedWords { TypedWords(words: words) }
        }
        .overlay {
            if !showing.elsewhere, showing.listing { KeyList() }
        }
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .paces(showing)
        .onAppear { focused = true }
    }
}

/// The stage past the last slide (PLAN 2.120), as Keynote's and PowerPoint's end of show: black,
/// saying the show has ended and how to leave it; going back shows the last slide.
struct ShowOver: View {
    static let ended = "The show has ended."
    #if os(macOS)
    private static let how = "Click to leave, or press ← for the last slide."
    #else
    private static let how = "Tap to leave, or swipe right for the last slide."
    #endif

    var body: some View {
        ZStack {
            Color.black
            VStack(spacing: 12) {
                Text(Self.ended).font(.title).foregroundStyle(.white)
                Text(Self.how).font(.title3).foregroundStyle(.white.opacity(0.75))
            }
            .multilineTextAlignment(.center)
            .padding(24)
        }
        .allowsHitTesting(false)
    }
}

/// The laser's dot (PLAN 2.105): red and glowing, as large as a room sees from its back, on a
/// view `across` points at its shorter side.
struct Laser: View {
    let across: CGFloat

    var body: some View {
        let red = Color(red: 1, green: 0.17, blue: 0.17)
        Circle()
            .fill(red)
            .frame(width: max(12, across * 0.014), height: max(12, across * 0.014))
            .shadow(color: red.opacity(0.75), radius: 5)
            .shadow(color: red.opacity(0.4), radius: 14)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

/// A slide's number as the presenter types it, or a slide the deck lacks, at the foot of the view.
struct TypedWords: View {
    let words: String

    var body: some View {
        Text(words)
            .font(.title2.monospacedDigit())
            .foregroundStyle(.white)
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .background(.black.opacity(0.85), in: RoundedRectangle(cornerRadius: 10))
            .padding(.bottom, 28)
            .accessibilityIdentifier("typed")
    }
}

/// The presenter's keys, listed (PLAN 2.105), as the browser's player lists them: ? shows and hides
/// them, and Escape puts them away.
struct KeyList: View {
    #if os(macOS)
    private static let laser = "The laser, where the pointer is over the slide"
    private static let back = "A key or a click brings back a black or white screen."
    #else
    private static let laser = "The laser, where the pointer is; or touch and hold the slide"
    private static let back = "A key or a tap brings back a black or white screen."
    #endif
    private struct Row: Hashable {
        let keys: String
        let does: String
    }
    private static let rows = [
        Row(keys: "→  Space  Return", does: "Go on: a cue playing finishes, then the next plays"),
        Row(keys: "←  Delete", does: "Go back"),
        Row(keys: "Home  End", does: "The first slide, the last"),
        Row(keys: "A slide’s number, then Return", does: "That slide"),
        Row(keys: "B or .", does: "A black screen, and the slide again"),
        Row(keys: "W or ,", does: "A white screen, and the slide again"),
        Row(keys: "L", does: laser),
        Row(keys: "Escape", does: "End the show"),
        Row(keys: "+  −", does: "The notes larger, or smaller, in the presenter's view"),
        Row(keys: "N", does: "The slides, to go to one, in the presenter's view"),
        Row(keys: "?", does: "These keys"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Keys").font(.title2.bold())
            Grid(alignment: .leading, horizontalSpacing: 24, verticalSpacing: 8) {
                ForEach(Self.rows, id: \.self) { row in
                    GridRow {
                        Text(row.keys).fontWeight(.semibold)
                        Text(row.does)
                    }
                }
            }
            Text(Self.back).foregroundStyle(.secondary)
        }
        .font(.title3)
        .padding(24)
        .floating(in: RoundedRectangle(cornerRadius: 14))
        .environment(\.colorScheme, .dark)
        .frame(maxWidth: 640)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("keys")
    }
}

/// The presenter's view: the state shown as the audience sees it, the next at rest, the state's
/// notes, where it is in the deck, the time since the deck began, which Pause stops and runs on
/// and Reset puts back to 0:00, and the time of day (PLAN 2.121). Side by side where it is wide;
/// where it is tall, as an iPad held upright, the next and the notes under the state shown.
/// A click or a tap on the state shown goes on, as one on the stage does; on the iPad, the stage's
/// swipes and pinch work on it too, and End Show ends the show (PLAN 4.8). The presenter's keys
/// say here what they do (PLAN 2.105): the room's screen black or white, said over the slide
/// dimmed; the laser pointed from the slide here; a slide's number typed; and the keys, listed.
/// The end of the show is said over the slide too (PLAN 2.120).
struct PresenterView: View {
    let showing: Showing
    @FocusState private var focused: Bool
    @State private var notes = Notes.none
    @State private var next: CGImage?
    @Environment(\.displayScale) private var scale
    /// The notes' size, a step up or down, kept for the next show (lift-up USE-4).
    @AppStorage("presenter.notesSize") private var notesSize = NotesSize.standard
    /// Whether every slide is shown, to go to one (lift-up 10-24b USE-1).
    @State private var navigating = false

    private func sizeNotes(_ by: Int) {
        notesSize = NotesSize.stepped(notesSize, by: by)
    }

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
                            GeometryReader { shown in
                                ZStack {
                                    Color.clear
                                        .contentShape(Rectangle())
                                        .onTapGesture { if !showing.lifting { showing.on() } }
                                        // The pointer here points the laser on the room's slide.
                                        .onContinuousHover(coordinateSpace: .local) { phase in
                                            switch phase {
                                            case .active(let at): showing.point(Self.fraction(at, of: shown.size))
                                            case .ended: showing.point(nil)
                                            }
                                        }
                                        #if !os(macOS)
                                        .turns(showing) { Self.fraction($0, of: shown.size) }
                                        #endif
                                    if let blank = show.blank {
                                        Blanked(blank: blank)
                                    } else if show.over {
                                        Blanked(words: Blanked.over)
                                    }
                                    if let at = showing.pointer {
                                        Laser(across: min(shown.size.width, shown.size.height))
                                            .position(x: at.x * shown.size.width, y: at.y * shown.size.height)
                                    }
                                }
                            }
                        }
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(showing.reading)
                        .accessibilityValue(showing.progress)
                        .accessibilityIdentifier("presented")
                        .accessibilityAddTraits(.isButton)
                        .accessibilityAction { showing.on() }
                    }
                    HStack(spacing: 12) {
                        Text(showing.progress)
                        Button("Slides") { navigating = true }
                            .font(.body)
                            .help("Every slide, to go to one (N)")
                            .accessibilityIdentifier("navigate")
                        Spacer()
                        // The timer, paused and reset here, and beside it the time of day.
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            HStack(spacing: 12) {
                                Text(showing.stopwatch.reads(at: context.date))
                                    .monospacedDigit()
                                    .foregroundStyle(showing.stopwatch.paused ? .secondary : .primary)
                                    .accessibilityLabel("Timer")
                                    .accessibilityValue(showing.stopwatch.reads(at: context.date))
                                    .accessibilityIdentifier("timer")
                                Button(showing.stopwatch.paused ? "Resume" : "Pause") {
                                    showing.stopwatch.toggle()
                                }
                                .font(.body)
                                .accessibilityIdentifier("pause-timer")
                                Button("Reset") {
                                    showing.stopwatch.reset()
                                }
                                .font(.body)
                                .accessibilityIdentifier("reset-timer")
                                Text(context.date, format: .dateTime.hour().minute())
                                    .monospacedDigit()
                                    .foregroundStyle(.secondary)
                                    .accessibilityLabel("Time of day")
                                    .accessibilityIdentifier("time-of-day")
                            }
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
                    // What comes, as a person counts it: a slide, or a step of the one shown, as
                    // the browser's presenter view says it (lift-up USE-4).
                    if let coming = show.next {
                        Text(Words.slide(coming.state, in: show.slots))
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("next-name")
                    }
                    HStack(spacing: 6) {
                        Text("Notes").font(.headline).foregroundStyle(.secondary)
                        Spacer()
                        Button("A−") { sizeNotes(-1) }
                            .disabled(notesSize <= NotesSize.sizes[0])
                            .accessibilityLabel("Smaller notes")
                            .accessibilityIdentifier("notes-smaller")
                            .help("Smaller notes (−)")
                        Button("A+") { sizeNotes(1) }
                            .disabled(notesSize >= NotesSize.sizes[NotesSize.sizes.count - 1])
                            .accessibilityLabel("Larger notes")
                            .accessibilityIdentifier("notes-larger")
                            .help("Larger notes (+)")
                    }
                    .padding(.top, 8)
                    ScrollView {
                        NotesText(notes.blocks)
                            .font(.system(size: notesSize))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                            .accessibilityIdentifier("presenter-notes")
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
        // What the presenter's keys say: a slide's number typed, and the keys when ? lists them.
        .overlay(alignment: .bottom) {
            if let words = showing.typedWords { TypedWords(words: words) }
        }
        .overlay {
            if showing.listing { KeyList() }
        }
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        // The notes larger or smaller, here alone: the stage has none (lift-up USE-4).
        .onKeyPress(characters: CharacterSet(charactersIn: "+=")) { press in
            guard plain(press) else { return .ignored }
            sizeNotes(1)
            return .handled
        }
        .onKeyPress(characters: CharacterSet(charactersIn: "-")) { press in
            guard plain(press) else { return .ignored }
            sizeNotes(-1)
            return .handled
        }
        // Every slide, to go to one, as Keynote's navigator, here alone (lift-up 10-24b USE-1).
        .onKeyPress(characters: CharacterSet(charactersIn: "nN")) { press in
            guard plain(press) else { return .ignored }
            navigating = true
            return .handled
        }
        .sheet(isPresented: $navigating) {
            SlideNavigator(showing: showing) { navigating = false }
        }
        .onChange(of: navigating) { _, now in
            if !now { focused = true }
        }
        .paces(showing)
        .onAppear { focused = true }
        .task(id: show.index) {
            notes = show.slot.map { showing.notes($0.state) } ?? .none
            // Drawn away from the stage, so a step plays on while the next is drawn (lift-up PERF-2).
            if let coming = show.next {
                next = await showing.editor.drawingAway(coming.state, width: Int(360 * scale))
            } else {
                next = nil
            }
        }
    }

    /// `location` on the slide shown here, `size` big, as fractions of it; none off it.
    static func fraction(_ location: CGPoint, of size: CGSize) -> CGPoint? {
        guard size.width > 0, size.height > 0 else { return nil }
        let at = CGPoint(x: location.x / size.width, y: location.y / size.height)
        return (0...1).contains(at.x) && (0...1).contains(at.y) ? at : nil
    }
}

/// The presenter's slides (lift-up 10-24b USE-1), as Keynote's navigator: every slide drawn small at
/// its last state, the one the room sees marked. The arrows, Home, and End move through them, and
/// Return or a click goes to one, as its number typed goes there; N, Escape, or Done closes it.
struct SlideNavigator: View {
    let showing: Showing
    let close: () -> Void
    /// The slide marked, from 0.
    @State private var marked = 0
    @FocusState private var focused: Bool

    var body: some View {
        let slides = Slide.of(showing.show.slots)
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Slides").font(.title2.bold())
                Spacer()
                Button("Done", action: close).keyboardShortcut(.cancelAction)
            }
            ScrollViewReader { scroller in
                ScrollView {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: 14)], spacing: 14) {
                        ForEach(Array(slides.enumerated()), id: \.element.id) { n, slide in
                            NavigatedSlide(
                                editor: showing.editor, slide: slide, number: n + 1, marked: n == marked,
                                revision: showing.editor.revision
                            )
                            .id(n)
                            .onTapGesture { go(slide) }
                            .accessibilityAction { go(slide) }
                        }
                    }
                    .padding(4)
                }
                .onChange(of: marked) { _, now in scroller.scrollTo(now) }
            }
        }
        .padding(20)
        .frame(minWidth: 640, idealWidth: 860, minHeight: 420, idealHeight: 600)
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(keys: [.rightArrow, .downArrow]) { _ in
            marked = min(marked + 1, max(slides.count - 1, 0))
            return .handled
        }
        .onKeyPress(keys: [.leftArrow, .upArrow]) { _ in
            marked = max(marked - 1, 0)
            return .handled
        }
        .onKeyPress(.home) {
            marked = 0
            return .handled
        }
        .onKeyPress(.end) {
            marked = max(slides.count - 1, 0)
            return .handled
        }
        .onKeyPress(.return) {
            if slides.indices.contains(marked) { go(slides[marked]) }
            return .handled
        }
        .onKeyPress(characters: CharacterSet(charactersIn: "nN")) { press in
            guard plain(press) else { return .ignored }
            close()
            return .handled
        }
        .onAppear {
            focused = true
            let room = showing.show.slot?.slide
            marked = slides.firstIndex { $0.id == room } ?? 0
        }
        .accessibilityIdentifier("navigator")
    }

    private func go(_ slide: Slide) {
        if let first = slide.states.first { showing.navigate(to: first) }
        close()
    }
}

/// A slide in the presenter's navigator: drawn at its last state, its number under it, outlined
/// where it is marked.
private struct NavigatedSlide: View {
    let editor: DeckEditor
    let slide: Slide
    let number: Int
    let marked: Bool
    let revision: Int
    @State private var drawing: CGImage?
    @Environment(\.displayScale) private var scale

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Group {
                if let drawing {
                    Image(decorative: drawing, scale: scale).resizable().aspectRatio(contentMode: .fit)
                } else {
                    Rectangle().fill(.quaternary).aspectRatio(16 / 9, contentMode: .fit)
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .overlay {
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(marked ? Color.accentColor : Color.secondary.opacity(0.3), lineWidth: marked ? 3 : 1)
            }
            Text("\(number)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Slide \(number)")
        .accessibilityAddTraits(marked ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("navigated-\(slide.id)")
        .task(id: "\(slide.last)\u{1f}\(revision)") {
            drawing = await editor.drawingAway(slide.last, width: Int(200 * scale))
        }
    }
}

/// The presenter's slide while the room's screen is black or white (PLAN 2.105), or the show is over
/// (PLAN 2.120): dimmed, and said.
struct Blanked: View {
    let words: String
    #if os(macOS)
    private static let back = "A key or a click brings the slide back."
    static let over = "The room sees the end of the show. A click or → leaves it, and ← shows the last slide."
    #else
    private static let back = "A key or a tap brings the slide back."
    static let over = "The room sees the end of the show. A tap leaves it, and a swipe to the right shows the last slide."
    #endif

    init(blank: Blank) {
        words = "The room sees a \(blank.rawValue) screen. \(Self.back)"
    }

    init(words: String) {
        self.words = words
    }

    var body: some View {
        ZStack {
            Color.black.opacity(0.6)
            Text(words)
                .font(.title3)
                .multilineTextAlignment(.center)
                .foregroundStyle(.white)
                .padding(16)
                .background(.black.opacity(0.8), in: RoundedRectangle(cornerRadius: 10))
                .padding(24)
                .accessibilityIdentifier("blanked")
        }
        .allowsHitTesting(false)
    }
}
