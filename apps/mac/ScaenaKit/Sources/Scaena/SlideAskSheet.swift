#if os(macOS)
import AppKit
#endif
import ScaenaKit
import SwiftUI

/// What Write This Slide Again… or Add a Slide About… asks for (lift-up FUNC-1): the slide written
/// again in its place, or a new one after it, and that slide as a person counts it, `Slide 3`.
struct SlideAsking: Identifiable {
    let id = UUID()
    let place: SlidePlace
    let slide: String

    var again: Bool {
        if case .replace = place { return true }
        return false
    }
}

/// Write This Slide Again… and Add a Slide About… (lift-up FUNC-1), in the browser's words
/// (`web/src/slide-ask.ts`): what to change, or what the new slide is about, written by the model
/// the assistant uses with the user's key, which the sheet names. ⌘Return writes it, as the
/// assistant's Ask asks; it is asked once, and the slide its answer holds is written as the deck's
/// generator writes one, handed to `wrote` for one step of undo. An answer that holds no slide,
/// or a deck that would not take it, changes nothing, and the sheet says why. Cancel, or the sheet
/// put away any other way, stops the request on its way. With no key kept, it says where one goes.
struct SlideAskSheet: View {
    let asking: SlideAsking
    let editor: DeckEditor
    /// The slide written: the source it replaced, for one step of undo, and its first state.
    let wrote: (SlideWritten) -> Void

    @Environment(\.dismiss) private var dismiss
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #else
    /// The keys, on the iPad in a sheet: it has no Settings window (PLAN 4.2).
    @State private var keying = false
    #endif
    @State private var ask = ""
    @State private var status = ""
    @State private var writing: Task<Void, Never>?
    /// The model the assistant uses and its key, read as the sheet opens and as the person comes
    /// back from keeping one.
    @State private var who: (provider: Provider, model: String, key: String)?
    @State private var keysChanged = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(asking.again ? "Write \(asking.slide) again" : "Add a slide after \(asking.slide)")
                .font(.headline)
            TextField(
                asking.again ? "What to change (or leave it empty)" : "What the new slide is about", text: $ask,
                axis: .vertical
            )
            .lineLimit(3...6)
            .textFieldStyle(.roundedBorder)
            .accessibilityIdentifier("slide-ask-text")
            Text(by)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if !status.isEmpty {
                Text(status)
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("slide-ask-status")
            }
            HStack {
                if who == nil {
                    Button("Add a Key…", action: addKey)
                        .accessibilityIdentifier("slide-ask-key")
                }
                Spacer()
                Button("Cancel") {
                    writing?.cancel()
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                Button(asking.again ? "Write It Again" : "Write the Slide", action: write)
                    // ⌘Return: Return is a line of what is asked.
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(!ready)
                    .accessibilityIdentifier("slide-ask-go")
            }
        }
        .padding(20)
        .frame(minWidth: 420)
        .onAppear { who = Writer.byKey() }
        .onChange(of: keysChanged) { who = Writer.byKey() }
        // Put away while a slide is coming: its request is stopped, and nothing is written.
        .onDisappear { writing?.cancel() }
        #if os(macOS)
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

    /// Whether it can be written now: a key and a model, nothing on its way, and, for a new
    /// slide, what it is about.
    private var ready: Bool {
        who != nil && writing == nil
            && (asking.again || !ask.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }

    /// Who writes it, and where the words go; or where a key goes.
    private var by: String {
        guard let who else {
            #if os(macOS)
            return "The slide is written by the model the assistant uses, with your own key: keep one in Settings and choose a model in the assistant."
            #else
            return "The slide is written by the model the assistant uses, with your own key: add one with Add a Key, and choose a model in the assistant."
            #endif
        }
        let name = who.provider.name
        return "Written by \(who.model), with your \(name) key: what it is asked goes to \(name) and nowhere else."
    }

    private func addKey() {
        #if os(macOS)
        openSettings()
        #else
        keying = true
        #endif
    }

    private func write() {
        guard let who, ready else { return }
        let words = ask.trimmingCharacters(in: .whitespacesAndNewlines)
        status = "Asking \(who.model)…"
        writing = Task { @MainActor in
            defer { writing = nil }
            do {
                let written = try await editor.writeSlide(
                    asking.place, ask: words, provider: who.provider, model: who.model, key: who.key)
                guard !Task.isCancelled else { return }
                wrote(written)
                dismiss()
            } catch {
                // Nothing changed: the answer held no slide, the deck would not take it, or the
                // model was not reached; said in a person's words (PLAN 3.18).
                if !Task.isCancelled { status = "Nothing changed: \(Why.said(error, of: who.provider))" }
            }
        }
    }
}
