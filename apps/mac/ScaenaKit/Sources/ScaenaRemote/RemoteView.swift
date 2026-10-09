import SwiftUI

#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// The remote (PLAN 4.10), on an iPhone or an iPad: the presenters on the local network; one
/// chosen, the code its editor shows; then the show it plays: the slide as the audience sees it,
/// the next, the notes, where the show is and the time since it began, and Back and Next, a tap or
/// a swipe as on the stage. With no show playing, Play plays it.
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
                Text("\(place.index + 1) of \(place.count)")
                Spacer()
                if let began = place.began {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(elapsed(context.date, since: began)).monospacedDigit()
                    }
                }
            }
            .font(.headline)
            slide(place.slide)
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
                .accessibilityLabel("Slide \(place.index + 1) of \(place.count)")
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
                    Text(place.notes.isEmpty ? "No notes." : place.notes)
                        .frame(maxWidth: .infinity, alignment: .leading)
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
            Button("End Show", role: .destructive) { client.send(.end) }
        }
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
