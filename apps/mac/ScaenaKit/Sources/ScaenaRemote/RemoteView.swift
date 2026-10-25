import ScaenaNotes
import SwiftUI

#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// The remote (PLAN 4.10), on an iPhone or an iPad: the presenters on the local network; one
/// chosen, the code its editor shows; then the show it plays: the slide as the audience sees it,
/// the next, the notes, where the show is and the presenter's timer, and Back and Next, a tap or a
/// swipe as on the stage. With no show playing, Play plays it. What the room sees is mirrored: a
/// black or white screen and the end of the show, which the Show menu asks for too, with the timer
/// paused or reset and a slide by its number (lift-up FUNC-5).
public struct RemoteView: View {
    @State private var browser = RemoteBrowser()
    @State private var client: RemoteClient?
    @State private var chosen: RemotePresenter?
    @State private var code = ""

    public init() {}

    public var body: some View {
        NavigationStack {
            Group {
                if let client {
                    RemoteShow(client: client, leave: leave)
                } else if let chosen {
                    pairing(chosen)
                } else {
                    finding
                }
            }
        }
        .onAppear { browser.start() }
        .onDisappear {
            browser.stop()
            client?.leave()
        }
    }

    /// The presenters nearby.
    private var finding: some View {
        List {
            Section {
                if browser.presenters.isEmpty {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text("Looking for a Mac or an iPad presenting…")
                    }
                    .foregroundStyle(.secondary)
                }
                ForEach(browser.presenters) { presenter in
                    Button(presenter.name) {
                        code = ""
                        chosen = presenter
                    }
                }
            } header: {
                Text("Presenting Nearby")
            } footer: {
                Text("On the Mac or the iPad, choose Remote… in the Play menu, then pick it here and enter the code it shows.")
            }
            if let trouble = browser.trouble {
                Text(trouble).foregroundStyle(.red)
            }
        }
        .navigationTitle("Remote")
    }

    /// The code the presenter's editor shows.
    private func pairing(_ presenter: RemotePresenter) -> some View {
        Form {
            Section {
                TextField("Code", text: $code)
                    .font(.system(size: 34, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    #if os(iOS)
                    .keyboardType(.numberPad)
                    #endif
                    .onSubmit { join(presenter) }
                    .accessibilityIdentifier("remote-code")
                Button("Join") { join(presenter) }
                    .disabled(code.count != 4)
            } header: {
                Text(presenter.name)
            } footer: {
                Text("The four digits its Remote… shows.")
            }
        }
        .navigationTitle("Enter the Code")
        .toolbar {
            Button("Cancel") { chosen = nil }
        }
    }

    private func join(_ presenter: RemotePresenter) {
        guard code.count == 4 else { return }
        client = RemoteClient(presenter.endpoint, code: code)
    }

    private func leave() {
        client?.leave()
        client = nil
        chosen = nil
    }
}

/// The show a remote is joined to.
private struct RemoteShow: View {
    let client: RemoteClient
    let leave: () -> Void
    /// Whether Go to Slide… asks for a number, and the number typed.
    @State private var going = false
    @State private var number = ""

    var body: some View {
        Group {
            switch client.status {
            case .joining:
                ProgressView("Joining…")
            case .lost(let why):
                ContentUnavailableView {
                    Label("Not Joined", systemImage: "wifi.exclamationmark")
                } description: {
                    Text(why)
                } actions: {
                    Button("Back", action: leave)
                }
            case .joined:
                if let place = client.place, place.showing {
                    playing(place)
                } else {
                    waiting
                }
            }
        }
        .toolbar {
            Button("Leave", action: leave)
        }
        #if os(iOS)
        // The screen kept on while the remote drives a show.
        .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
        .onDisappear { UIApplication.shared.isIdleTimerDisabled = false }
        #endif
    }

    /// Joined, with no show playing.
    private var waiting: some View {
        ContentUnavailableView {
            Label(client.place?.deck ?? "Joined", systemImage: "play.rectangle")
        } description: {
            Text("The show has not begun.")
        } actions: {
            Button("Play") { client.send(.play) }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("remote-play")
        }
    }

    /// The show as it plays.
    private func playing(_ place: RemotePlace) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(place.said)
                Spacer()
                // The presenter's timer, as their view reads it, paused where it is; from a presenter
                // that sends none, the time since the show began.
                if let timer = place.timer {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(timer.reads(at: context.date)).monospacedDigit()
                            .foregroundStyle(timer.paused ? .secondary : .primary)
                            .accessibilityLabel(timer.paused ? "Timer, paused" : "Timer")
                            .accessibilityValue(timer.reads(at: context.date))
                    }
                } else if let began = place.began {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(elapsed(context.date, since: began)).monospacedDigit()
                    }
                }
            }
            .font(.headline)
            screen(place)
                .contentShape(Rectangle())
                .onTapGesture { client.send(.on) }
                .gesture(
                    DragGesture(minimumDistance: 24)
                        .onEnded { drag in
                            let across = drag.translation.width
                            guard abs(across) > max(abs(drag.translation.height), 48) else { return }
                            client.send(across < 0 ? .on : .back)
                        }
                )
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(seen(place))
                .accessibilityAddTraits(.isButton)
                .accessibilityAction { client.send(.on) }
                .accessibilityIdentifier("remote-slide")
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Next").font(.caption).foregroundStyle(.secondary)
                    if place.next != nil {
                        slide(place.next).frame(maxWidth: 160)
                    } else {
                        Text("The end").font(.caption).foregroundStyle(.secondary)
                    }
                }
                ScrollView {
                    NotesText(text: place.notes, blocks: place.read)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("remote-notes")
                }
            }
            HStack(spacing: 12) {
                Button {
                    client.send(.back)
                } label: {
                    Label("Back", systemImage: "chevron.left").frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("remote-back")
                Button {
                    client.send(.on)
                } label: {
                    Label("Next", systemImage: "chevron.right").frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("remote-next")
            }
        }
        .padding()
        .navigationTitle(place.deck)
        .toolbar {
            Menu("Show") {
                Button(place.blank == .black ? "Show the Slide" : "Black Screen") { client.send(.black) }
                Button(place.blank == .white ? "Show the Slide" : "White Screen") { client.send(.white) }
                Button("Go to Slide…") {
                    number = ""
                    going = true
                }
                Divider()
                Button(place.timer?.paused == true ? "Resume Timer" : "Pause Timer") { client.send(.pauseTimer) }
                Button("Reset Timer") { client.send(.resetTimer) }
            }
            .accessibilityIdentifier("remote-show-menu")
            Button("End Show", role: .destructive) { client.send(.end) }
        }
        .alert("Go to Slide", isPresented: $going) {
            TextField("Number", text: $number)
                #if os(iOS)
                .keyboardType(.numberPad)
                #endif
            Button("Go") {
                if let slide = Int(number.trimmingCharacters(in: .whitespaces)) { client.go(slide: slide) }
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    /// What the room sees, as the stage shows it: the end of the show, a black or white screen, or
    /// the slide.
    @ViewBuilder private func screen(_ place: RemotePlace) -> some View {
        if place.over {
            covered(.black, "End of the show")
        } else if let blank = place.blank {
            covered(blank == .black ? .black : .white, blank == .black ? "Black screen" : "White screen")
        } else {
            slide(place.slide)
        }
    }

    /// The slide out of sight under `color`, saying so in small words, as the stage's 16:9.
    private func covered(_ color: Color, _ words: String) -> some View {
        RoundedRectangle(cornerRadius: 6)
            .fill(color)
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.quaternary))
            .overlay {
                Text(words).font(.caption).foregroundStyle(color == .black ? Color.white.opacity(0.7) : Color.black.opacity(0.55))
            }
            .aspectRatio(16 / 9, contentMode: .fit)
    }

    /// What VoiceOver hears of the screen: where the show is, and what the room sees there.
    private func seen(_ place: RemotePlace) -> String {
        let at = place.said
        if place.over { return "\(at), end of the show" }
        return place.blank.map { "\(at), \($0.rawValue) screen" } ?? at
    }

    /// A state drawn at rest, as the presenter sent it.
    @ViewBuilder private func slide(_ png: Data?) -> some View {
        if let image = picture(png) {
            image.resizable().aspectRatio(contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 6))
        } else {
            RoundedRectangle(cornerRadius: 6).fill(.quaternary).aspectRatio(16 / 9, contentMode: .fit)
        }
    }

    private func picture(_ png: Data?) -> Image? {
        guard let png else { return nil }
        #if canImport(UIKit)
        return UIImage(data: png).map { Image(uiImage: $0) }
        #else
        return NSImage(data: png).map { Image(nsImage: $0) }
        #endif
    }

    private func elapsed(_ now: Date, since began: Date) -> String {
        let seconds = max(Int(now.timeIntervalSince(began)), 0)
        return String(format: "%ld:%02ld", seconds / 60, seconds % 60)
    }
}
