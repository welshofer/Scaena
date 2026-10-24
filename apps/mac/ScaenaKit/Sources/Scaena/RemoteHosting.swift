#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ImageIO
import Observation
import ScaenaKit
import ScaenaNotes
import ScaenaRemote
import SwiftUI
import UniformTypeIdentifiers

/// The remote's host on this Mac or iPad (PLAN 4.10, ADR-0025): while Remote… is on, a deck is
/// offered on the local network under this device's name, to a remote that has the code its sheet
/// shows. It tells each remote where the show is as it goes: the state, at rest or in its cue, its
/// notes, and it and the next drawn at rest. It does what a remote asks: play the deck from the
/// slide the editor shows, go on, go back, the first and the last state, and end the show.
@MainActor @Observable
final class RemoteHost {
    /// The app's one host: a show plays one at a time.
    static let shared = RemoteHost()

    /// The code a remote joins with, while the host is on.
    private(set) var code: String?
    /// What the remote finds this device as: its name alone, never the deck's (lift-up SEC-5).
    private(set) var name = ""
    /// How many remotes are joined.
    private(set) var joined = 0
    /// How many wrong codes have been tried since the code was made.
    private(set) var tried = 0
    /// Whether the code was made anew after wrong ones, since the host was turned on.
    private(set) var recoded = false
    private(set) var trouble: String?
    var on: Bool { code != nil }

    @ObservationIgnored private var server: RemoteServer?
    /// The deck a remote's Play plays, and the slide it plays from.
    @ObservationIgnored private weak var editor: DeckEditor?
    @ObservationIgnored private var from: () -> String? = { nil }
    @ObservationIgnored private var deck = ""
    /// The show that plays, if one does.
    @ObservationIgnored private weak var showing: Showing?
    /// The place last sent, as a key: the state, whether at rest, the deck's revision, the screen
    /// black or white, and whether the show is over; and the presenter's timer as it was sent.
    @ObservationIgnored private var sent: String?
    @ObservationIgnored private var sentTimer: Stopwatch?
    /// The states drawn for remotes, as PNGs, by the deck's revision.
    @ObservationIgnored private var drawn: [String: Data] = [:]

    private init() {}

    /// Offer `editor`'s deck, called `deck`, to remotes, played from the slide `from` names.
    func start(_ editor: DeckEditor, deck: String, from: @escaping () -> String?) {
        stop()
        let name = Remote.serviceName(device: Self.device)
        do {
            let server = try RemoteServer(name: name)
            server.command = { [weak self] command in self?.perform(command) }
            server.slide = { [weak self] number in self?.showing?.go(slide: number) }
            server.joined = { [weak self] count in self?.joined = count }
            server.tried = { [weak self] count in self?.tried = count }
            server.recoded = { [weak self] code in
                self?.code = code
                self?.recoded = true
            }
            // What the network says of it is not a person's words (PLAN 3.18): the window says
            // what to do.
            server.failed = { [weak self] _ in self?.trouble = Self.unoffered }
            self.server = server
            self.editor = editor
            self.from = from
            self.deck = deck
            self.name = name
            code = server.code
            trouble = nil
            follow(Presenting.current)
        } catch {
            trouble = Self.unoffered
        }
    }

    /// Why remotes cannot join, in a person's words.
    private static var unoffered: String {
        "The deck can't be offered to a remote. Check that this \(Words.device) is on a network, and try again."
    }

    /// Offer the deck no more, and let every remote go.
    func stop() {
        server?.stop()
        server = nil
        code = nil
        joined = 0
        tried = 0
        recoded = false
        drawn = [:]
    }

    /// Follow `showing`, the show that plays now, or none.
    func follow(_ showing: Showing?) {
        self.showing = showing
        sent = nil
        tell()
        if let showing { watch(showing) }
    }

    /// Each change of the show, told: once a change comes, it is told and watched for again.
    private func watch(_ showing: Showing) {
        withObservationTracking {
            _ = showing.show
            // The timer paused, run on, or reset is told as the show is (lift-up FUNC-5).
            _ = showing.stopwatch
        } onChange: { [weak self, weak showing] in
            Task { @MainActor in
                guard let self, let showing, self.showing === showing else { return }
                self.tell()
                self.watch(showing)
            }
        }
    }

    /// Tell the remotes where the show is, where that changed: a state, its cue played or at rest.
    private func tell() {
        guard let server else { return }
        guard let showing, let slot = showing.show.slot else {
            if sent != "" { server.tell(.waiting(deck)) }
            sent = ""
            return
        }
        let show = showing.show
        let rest = !show.playhead.playing
        let revision = showing.editor.revision
        let key = "\(show.index)\u{1f}\(rest)\u{1f}\(revision)\u{1f}\(show.blank?.rawValue ?? "")\u{1f}\(show.over)"
        guard key != sent || showing.stopwatch != sentTimer else { return }
        sent = key
        sentTimer = showing.stopwatch
        let ms = show.playhead.ms.isFinite ? max(show.playhead.ms, 0) : 0
        let notes = showing.notes(slot.state)
        let now = Date()
        // What the room sees, mirrored (lift-up FUNC-5): a black or white screen, the end, and the
        // presenter's own timer, paused where it is.
        let timer = RemoteTimer(
            elapsed: showing.stopwatch.elapsed(at: now), paused: showing.stopwatch.paused, at: now)
        server.tell(
            RemotePlace(
                deck: deck, showing: true, state: slot.state, index: show.index, count: show.slots.count,
                ms: rest ? 0 : ms, rest: rest, notes: notes.text, read: notes.blocks,
                slide: png(slot.state, showing.editor), next: show.next.flatMap { png($0.state, showing.editor) },
                began: showing.began, blank: show.blank.flatMap { RemoteScreen(rawValue: $0.rawValue) },
                over: show.over, timer: timer))
    }

    /// `state` drawn at rest for a remote, as a PNG.
    private func png(_ state: String, _ editor: DeckEditor) -> Data? {
        let key = "\(editor.revision)\u{1f}\(state)"
        if let kept = drawn[key] { return kept }
        guard let image = editor.drawing(state, width: 960) else { return nil }
        let data = NSMutableData()
        guard let made = CGImageDestinationCreateWithData(data as CFMutableData, UTType.png.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(made, image, nil)
        guard CGImageDestinationFinalize(made) else { return nil }
        drawn[key] = data as Data
        return data as Data
    }

    /// What a remote asks.
    private func perform(_ command: RemoteCommand) {
        if let showing {
            switch command {
            case .on: showing.on()
            case .back: showing.back()
            case .first: showing.first()
            case .last: showing.last()
            case .end: showing.ended()
            case .black: showing.blanking(.black)
            case .white: showing.blanking(.white)
            case .pauseTimer: showing.stopwatch.toggle()
            case .resetTimer: showing.stopwatch.reset()
            case .play: break
            }
        } else if command == .play, let editor, !editor.slots.isEmpty {
            Presenting.play(editor, from: from())
        }
    }

    /// What this device calls itself.
    private static var device: String {
        #if os(macOS)
        return Host.current().localizedName ?? "Mac"
        #else
        return UIDevice.current.name
        #endif
    }
}

/// Remote… (PLAN 4.10): let a remote on an iPhone or an iPad drive this deck's show, with the code
/// shown here, in the editor, where the audience does not see it.
struct RemoteSheet: View {
    let editor: DeckEditor
    /// The slide a remote's Play plays from.
    let from: () -> String?
    @Environment(\.documentConfiguration) private var document
    @Environment(\.dismiss) private var dismiss
    private var host: RemoteHost { .shared }

    var body: some View {
        VStack(spacing: 16) {
            Toggle("Let a remote play this deck", isOn: Binding(get: { host.on }, set: { turn($0) }))
                .toggleStyle(.switch)
                .accessibilityIdentifier("remote-on")
            if let code = host.code {
                Text(code)
                    .font(.system(size: 56, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .accessibilityLabel("Code \(code.map(String.init).joined(separator: " "))")
                    .accessibilityIdentifier("remote-code")
                Text(joined)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
                if let guessed {
                    Text(guessed)
                        .multilineTextAlignment(.center)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("remote-guessed")
                }
            } else {
                Text(
                    "A remote on an iPhone or an iPad plays the show from wherever you stand, and shows the notes and what comes next."
                )
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            }
            if let trouble = host.trouble {
                Text(trouble).foregroundStyle(.red)
            }
            Button("Done") { dismiss() }
                .keyboardShortcut(.defaultAction)
        }
        .padding(24)
        .frame(minWidth: 360, maxWidth: 440)
    }

    private var joined: String {
        switch host.joined {
        case 0: "On an iPhone or an iPad on this network, open the remote, choose \(host.name), and enter this code."
        case 1: "A remote is joined."
        default: "\(host.joined) remotes are joined."
        }
    }

    /// Wrong codes tried, and the code made anew after enough of them (lift-up SEC-5): what a
    /// presenter should know, since someone on the network may be guessing.
    private var guessed: String? {
        switch (host.recoded, host.tried) {
        case (false, 0): nil
        case (false, 1): "A wrong code was tried."
        case (false, let tried): "\(tried) wrong codes were tried."
        case (true, 0): "The code was changed after wrong codes were tried. Enter this one; a remote joined stays joined."
        case (true, let tried):
            "The code was changed after wrong codes were tried, and \(tried) more since. Turn the remote off if no one you know is trying."
        }
    }

    private func turn(_ on: Bool) {
        if on {
            let deck = document?.fileURL?.deletingPathExtension().lastPathComponent ?? "Deck"
            host.start(editor, deck: deck, from: from)
        } else {
            host.stop()
        }
    }
}
