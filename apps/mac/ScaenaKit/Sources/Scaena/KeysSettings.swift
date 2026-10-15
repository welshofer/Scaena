import ScaenaKit
import SwiftUI

/// The keys, in Settings on the Mac and a sheet on the iPad (PLAN 3.6, 4.9; ADR-0006, ADR-0022),
/// as a settings window shows an account: a section for each provider that says whether its key
/// works. A key kept shows masked, whose it is and which, and is checked with its provider each
/// time the settings show it. A key typed or pasted is checked before it is kept: its form as it
/// is typed, then by its provider, which lists the models it can use or refuses it; it is kept
/// when it works, or, where the provider cannot be asked, when the user says so. Each key stays in
/// the Keychain and goes to its provider alone.
struct KeysSettings: View {
    var body: some View {
        Form {
            Section {
                Label {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Your Own Keys").font(.headline)
                        Text(
                            "The assistant, the writing tools, New from a Prompt, and Make a Picture ask the models you choose with your own key. Each key stays in your Keychain and goes only to its provider. It is never written into a deck."
                        )
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    }
                } icon: {
                    Image(systemName: "key.horizontal.fill")
                        .font(.title2)
                        .foregroundStyle(.tint)
                }
            }
            ForEach(Provider.allCases) { provider in
                ProviderKey(provider: provider)
            }
        }
        .formStyle(.grouped)
        #if os(macOS)
        .frame(width: 560)
        .frame(minHeight: 560, idealHeight: 680)
        #else
        .navigationTitle("Keys")
        #endif
    }
}

#if !os(macOS)
/// The keys as a sheet on the iPad (PLAN 4.9), as a presentation app's settings sheet: its title
/// and Done alone. Nothing comes before it, so it has no way back: the document's own way back to
/// the browser showed a second way out beside Done.
struct KeysSheet: View {
    let done: () -> Void

    var body: some View {
        NavigationStack {
            KeysSettings()
                .navigationBarTitleDisplayMode(.inline)
                .navigationBarBackButtonHidden(true)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done", action: done)
                    }
                }
        }
    }
}
#endif

/// One provider's key: whether it works, the key masked or a field to give one, and what is done
/// with it.
private struct ProviderKey: View {
    let provider: Provider
    private let keychain = Keychain()
    /// The key the Keychain keeps for the provider.
    @State private var kept: String?
    /// What is typed or pasted in the field.
    @State private var typed = ""
    /// Whether a key kept is being replaced, so the field shows in its place.
    @State private var replacing = false
    /// Whether the field shows what is typed.
    @State private var revealed = false
    @State private var checked: Checked = .idle
    @State private var removing = false
    @State private var failure: String?
    @FocusState private var focused: Bool

    /// Where the check of a key stands.
    enum Checked: Equatable {
        case idle
        case checking
        case done(KeyCheck.Verdict)
    }

    var body: some View {
        Section {
            heading
            if let kept, !replacing {
                keeping(kept)
            } else {
                entry
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(.red)
            }
        } footer: {
            Link("Get \(provider.withArticle) key", destination: provider.keysPage)
                .font(.callout)
        }
        .task { await load() }
        .confirmationDialog(
            "Remove the \(provider.name) key?", isPresented: $removing, titleVisibility: .visible
        ) {
            Button("Remove Key", role: .destructive, action: remove)
                .accessibilityIdentifier("remove-confirmed-\(provider.rawValue)")
        } message: {
            Text("It is taken out of your Keychain. \(provider.name)'s models can't be asked until you add another.")
        }
    }

    /// The provider, what its key is for, and whether its key works.
    private var heading: some View {
        HStack(spacing: 10) {
            Monogram(provider: provider)
            VStack(alignment: .leading, spacing: 1) {
                Text(provider.name).font(.headline)
                Text(provider.family).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Status(checked: checked, kept: kept != nil)
                .accessibilityIdentifier("status-\(provider.rawValue)")
        }
        .accessibilityElement(children: .combine)
    }

    /// The key kept, masked, with what is said of it, and what can be done with it.
    @ViewBuilder private func keeping(_ key: String) -> some View {
        LabeledContent("Key") {
            Text(KeyCheck.masked(key))
                .font(.body.monospaced())
                .foregroundStyle(.secondary)
                .accessibilityLabel("\(provider.name) key ending in \(String(key.suffix(4)))")
                .accessibilityIdentifier("kept-\(provider.rawValue)")
        }
        if case .done(let verdict) = checked, let said = verdict.said {
            Text(said)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        HStack {
            Button("Check Again") { Task { await check(key) } }
                .disabled(checked == .checking)
            Button("Replace…") {
                typed = ""
                revealed = false
                replacing = true
                focused = true
            }
            .accessibilityIdentifier("replace-\(provider.rawValue)")
            Spacer()
            Button("Remove…", role: .destructive) { removing = true }
                .accessibilityIdentifier("remove-\(provider.rawValue)")
        }
        #if !os(macOS)
        // A row of a form on the iPad takes a tap as every button's in it unless each is
        // borderless.
        .buttonStyle(.borderless)
        #endif
    }

    /// The field to give a key in, masked unless shown, with what its form says and the provider's
    /// answer, and the buttons that check and keep it.
    @ViewBuilder private var entry: some View {
        HStack(spacing: 8) {
            Group {
                if revealed {
                    TextField("Key", text: $typed, prompt: Text("Paste \(provider.withArticle) key"))
                } else {
                    SecureField("Key", text: $typed, prompt: Text("Paste \(provider.withArticle) key"))
                }
            }
            .labelsHidden()
            .font(.body.monospaced())
            .autocorrectionDisabled()
            #if !os(macOS)
            .textInputAutocapitalization(.never)
            .keyboardType(.asciiCapable)
            #endif
            .focused($focused)
            .onSubmit(submit)
            .onChange(of: typed) {
                // A key changed is checked again.
                if checked != .checking { checked = .idle }
            }
            .accessibilityIdentifier("key-\(provider.rawValue)")
            Button {
                revealed.toggle()
                focused = true
            } label: {
                Image(systemName: revealed ? "eye.slash" : "eye")
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.borderless)
            .help(revealed ? "Hide the key" : "Show the key")
            .accessibilityLabel(revealed ? "Hide Key" : "Show Key")
            .accessibilityIdentifier("reveal-\(provider.rawValue)")
        }
        if let hint = KeyCheck.hint(typed, for: provider) {
            Label(hint, systemImage: "info.circle")
                .font(.callout)
                .foregroundStyle(.orange)
                .accessibilityIdentifier("hint-\(provider.rawValue)")
        }
        if case .done(let verdict) = checked, !verdict.works, let said = verdict.said {
            Label(said, systemImage: verdict.symbol)
                .font(.callout)
                .foregroundStyle(verdict.tint)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("refusal-\(provider.rawValue)")
        }
        HStack {
            PasteButton(payloadType: String.self) { strings in
                guard let pasted = strings.first?.trimmingCharacters(in: .whitespacesAndNewlines) else { return }
                Task { @MainActor in typed = pasted }
            }
            .labelStyle(.titleAndIcon)
            if kept != nil {
                Button("Cancel") {
                    replacing = false
                    typed = ""
                    checked = .idle
                    Task { await load() }
                }
            }
            Spacer()
            if case .done(.unreachable) = checked {
                Button("Keep Without Checking") { keep(typed) }
                    .accessibilityIdentifier("keep-unchecked-\(provider.rawValue)")
            }
            Button(action: submit) {
                if checked == .checking {
                    ProgressView().controlSize(.small)
                } else {
                    Text("Check and Keep")
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(typed.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || checked == .checking)
            .accessibilityLabel("Check and Keep")
            .accessibilityIdentifier("keep-\(provider.rawValue)")
        }
        #if !os(macOS)
        .buttonStyle(.borderless)
        #endif
    }

    /// The key kept, read from the Keychain and checked with its provider.
    private func load() async {
        kept = keychain.key(for: provider)
        guard let kept else {
            checked = .idle
            return
        }
        await check(kept)
    }

    private func check(_ key: String) async {
        checked = .checking
        checked = .done(await KeyCheck.check(key, for: provider))
    }

    /// The key typed checked with its provider, and kept where it works.
    private func submit() {
        let key = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty, checked != .checking else { return }
        Task {
            checked = .checking
            let verdict = await KeyCheck.check(key, for: provider)
            checked = .done(verdict)
            if verdict.works { keep(key) }
        }
    }

    private func keep(_ key: String) {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        do {
            try keychain.set(key, for: provider)
            kept = key
            typed = ""
            revealed = false
            replacing = false
            failure = nil
        } catch {
            failure = Why.said(error)
        }
    }

    private func remove() {
        do {
            try keychain.set(nil, for: provider)
            kept = nil
            typed = ""
            replacing = false
            checked = .idle
            failure = nil
        } catch {
            failure = Why.said(error)
        }
    }
}

/// Whether a provider's key works, as a badge: a key that works, refused, not checked, or none.
private struct Status: View {
    let checked: ProviderKey.Checked
    let kept: Bool

    var body: some View {
        Group {
            switch checked {
            case .checking:
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small)
                    Text("Checking…")
                }
                .foregroundStyle(.secondary)
            case .done(let verdict):
                Label {
                    Text(verdict.badge)
                } icon: {
                    Image(systemName: verdict.symbol).foregroundStyle(verdict.tint)
                }
            case .idle:
                Label {
                    Text(kept ? "Kept" : "No Key")
                } icon: {
                    Image(systemName: kept ? "key.fill" : "circle.dashed").foregroundStyle(.secondary)
                }
                .foregroundStyle(.secondary)
            }
        }
        .font(.callout)
        .labelStyle(.titleAndIcon)
    }
}

/// A provider's initial on a tinted tile, as a settings window marks an account.
private struct Monogram: View {
    let provider: Provider

    var body: some View {
        Text(String(provider.name.prefix(1)))
            .font(.system(size: 14, weight: .semibold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: 28, height: 28)
            .background(provider.tint.gradient, in: .rect(cornerRadius: 7, style: .continuous))
            .accessibilityHidden(true)
    }
}

extension KeyCheck.Verdict {
    /// What the badge says.
    var badge: String {
        switch self {
        case .accepted(let models?): models == 1 ? "Works · 1 model" : "Works · \(models) models"
        case .accepted: "Works"
        case .refused: "Refused"
        case .unreachable: "Not Checked"
        }
    }

    /// The sentences that say why a key does not work, or could not be checked, in a person's
    /// words (PLAN 3.18), as the browser's key field says them.
    var said: String? {
        switch self {
        case .accepted: nil
        case .refused(let why): why
        case .unreachable(let why): "The key couldn't be checked. \(why)"
        }
    }

    var symbol: String {
        switch self {
        case .accepted: "checkmark.circle.fill"
        case .refused: "xmark.octagon.fill"
        case .unreachable: "exclamationmark.triangle.fill"
        }
    }

    var tint: Color {
        switch self {
        case .accepted: .green
        case .refused: .red
        case .unreachable: .orange
        }
    }
}

extension Provider {
    /// The models its key asks.
    fileprivate var family: String {
        switch self {
        case .anthropic: "Claude"
        case .openai: "GPT and pictures"
        case .gemini: "Gemini and pictures"
        }
    }

    /// Its tile's tint.
    fileprivate var tint: Color {
        switch self {
        case .anthropic: Color(red: 0.80, green: 0.47, blue: 0.36)
        case .openai: Color(white: 0.2)
        case .gemini: Color(red: 0.26, green: 0.46, blue: 0.93)
        }
    }
}
