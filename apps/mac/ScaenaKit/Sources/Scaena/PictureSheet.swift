#if os(macOS)
import AppKit
#endif
import ScaenaKit
import SwiftUI

/// Make a Picture (PLAN 2.107): what the picture shows, in the person's words, an image's own
/// written in; as an illustration or a photograph; and who makes it, by the key kept for it. Make
/// asks the best image model the key lists for it, in the theme's palette, framed as the place it
/// goes is, as the browser's dialog does, and hands it to `put`: one step to undo. The sheet says
/// why where none was made, and Cancel, or the sheet put away any other way (swiped down on the
/// iPad), drops a picture still coming, as the browser's dialog closed does. With no key kept for
/// the provider chosen, it says where one goes, and reads the keys again as the person comes back
/// from keeping one.
struct PictureSheet: View {
    let target: PictureTarget
    /// The deck's theme, its JSON: its palette colors the picture.
    let theme: String
    /// The picture made, put where it was asked for.
    let put: (MadePicture) -> Void

    @Environment(\.dismiss) private var dismiss
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #else
    /// The keys, on the iPad in a sheet: it has no Settings window (PLAN 4.2).
    @State private var keying = false
    #endif
    @State private var subject: String
    @State private var look = "illustration"
    @State private var provider: Provider = .openai
    @State private var status = ""
    @State private var making: Task<Void, Never>?
    /// Bumped as the keys sheet closes, or, on the Mac, as a window comes back from Settings, so
    /// that the keys are read again.
    @State private var keysChanged = 0

    private let keychain = Keychain()

    init(target: PictureTarget, theme: String, put: @escaping (MadePicture) -> Void) {
        self.target = target
        self.theme = theme
        self.put = put
        _subject = State(initialValue: target.shows ?? "")
    }

    /// The image providers whose key is kept.
    private var kept: [Provider] {
        _ = keysChanged
        return [.openai, .gemini].filter { keychain.key(for: $0) != nil }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Make a Picture")
                .font(.headline)
            TextField("A lighthouse on a rocky point at dusk, seen from the sea", text: $subject, axis: .vertical)
                .lineLimit(3...6)
                .textFieldStyle(.roundedBorder)
                .accessibilityLabel("What it shows")
                .accessibilityIdentifier("picture-subject")
            Picker("As", selection: $look) {
                Text("An Illustration").tag("illustration")
                Text("A Photograph").tag("photo")
            }
            .pickerStyle(.segmented)
            Picker("Made by", selection: $provider) {
                Text("OpenAI").tag(Provider.openai)
                Text("Gemini").tag(Provider.gemini)
            }
            Text(placed)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(who)
                .font(.caption)
                .foregroundStyle(.secondary)
            if !status.isEmpty {
                Text(status)
                    .font(.callout)
                    .accessibilityIdentifier("picture-status")
            }
            HStack {
                if !kept.contains(provider) {
                    Button("Add a Key…", action: addKey)
                }
                Spacer()
                Button("Cancel") {
                    making?.cancel()
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                Button("Make", action: make)
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        making != nil || !kept.contains(provider)
                            || subject.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(20)
        .frame(minWidth: 420)
        .onAppear {
            if !kept.contains(provider), let first = kept.first { provider = first }
        }
        // Put away while a picture is coming: it is dropped, and nothing is put anywhere.
        .onDisappear { making?.cancel() }
        #if os(macOS)
        // Back from Settings, where Add a Key… sent the person: the keys kept are read again.
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            keysChanged += 1
        }
        #else
        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
            NavigationStack {
                KeysSettings()
                    .toolbar { Button("Done") { keying = false } }
            }
        }
        #endif
    }

    /// Where the picture goes, framed as its place is.
    private var placed: String {
        if target.node != nil { return "It takes the place of the picture selected, \(target.frame) as its place is." }
        if target.slot != nil {
            return "It fills the empty place for a picture on this slide, \(target.frame) as that place is."
        }
        return "It goes on this slide where you last clicked, \(target.frame)."
    }

    /// Who makes it, and where the words go; or where a key goes.
    private var who: String {
        guard kept.contains(provider) else {
            #if os(macOS)
            return "No \(provider.name) key is kept: add one in Settings."
            #else
            return "No \(provider.name) key is kept: add one with Add a Key."
            #endif
        }
        return "Made by \(provider.name)'s best image model your key lists: what you ask goes to \(provider.name) and nowhere else."
    }

    private func addKey() {
        #if os(macOS)
        openSettings()
        #else
        keying = true
        #endif
    }

    private func make() {
        guard let key = keychain.key(for: provider) else { return }
        let painter = Generator.Painter(provider: provider, key: key, look: look)
        let shows = subject.trimmingCharacters(in: .whitespacesAndNewlines)
        status = "Making the picture with \(provider.name)… (up to a minute)"
        making = Task { @MainActor in
            defer { making = nil }
            do {
                let made = try await Generator().picture(subject: shows, theme: theme, size: target.size, painter: painter)
                guard !Task.isCancelled else { return }
                put(made)
                dismiss()
            } catch {
                // Why none was made, in a person's words (PLAN 3.18): the key not accepted, the
                // provider's safety system refusing, no way to it.
                if !Task.isCancelled { status = "No picture was made. \(Why.said(error, of: painter.provider))" }
            }
        }
    }
}
