import ScaenaKit
import SwiftUI

/// The assistant beside the canvas (PLAN 3.6, as the browser's, PLAN 2.6): the user's question,
/// with their own key from the Keychain, to the model they chose; each call the model makes, what
/// it came to, and each frame it drew; and the model's answer. Each question begins with what the
/// window shows: the state shown and the node selected. Each edit it makes shows as it is made,
/// and Undo takes it back. A provider is listed once it has a key, which Settings keeps.
struct AssistantPanel: View {
    let assistant: Assistant
    /// What the window shows: what "this" in a question means.
    let seeing: () -> Seeing?
    @AppStorage("assistant.provider") private var provider = Provider.anthropic
    @AppStorage("assistant.model.anthropic") private var anthropicModel = ""
    @AppStorage("assistant.model.openai") private var openaiModel = ""
    @AppStorage("assistant.model.gemini") private var geminiModel = ""
    @State private var question = ""
    @State private var models: [String] = []
    @State private var listing: String?
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #else
    /// The keys, on the iPad in a sheet: it has no Settings window (PLAN 4.2).
    @State private var keying = false
    #endif
    /// Whether the provider has no key kept: the panel offers to add one.
    @State private var keyless = false
    /// Bumped as the keys sheet closes, so that the models are listed again with the key kept.
    @State private var keysChanged = 0

    private let keychain = Keychain()

    private var model: Binding<String> {
        switch provider {
        case .anthropic: $anthropicModel
        case .openai: $openaiModel
        case .gemini: $geminiModel
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            conversation
            Divider()
            asking
        }
        .task(id: "\(provider.rawValue)\u{1f}\(keysChanged)") { await list() }
        #if !os(macOS)
        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
            NavigationStack {
                KeysSettings()
                    .toolbar { Button("Done") { keying = false } }
            }
        }
        #endif
    }

    /// The provider and its model.
    private var header: some View {
        HStack(spacing: 8) {
            Picker("Provider", selection: $provider) {
                ForEach(Provider.allCases) { provider in
                    Text(provider.name).tag(provider)
                }
            }
            .labelsHidden()
            .fixedSize()
            Picker("Model", selection: model) {
                if !models.contains(model.wrappedValue) {
                    Text(model.wrappedValue.isEmpty ? "Choose a model" : model.wrappedValue).tag(model.wrappedValue)
                }
                ForEach(models, id: \.self) { name in
                    Text(name).tag(name)
                }
            }
            .labelsHidden()
            Spacer(minLength: 0)
            Menu {
                Button("New conversation") { assistant.forget() }
                    .disabled(assistant.working)
                Button("Keys…", action: openKeys)
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .accessibilityLabel("Assistant Options")
            .accessibilityIdentifier("assistant-options")
        }
        .padding(8)
    }

    /// Each question, what the model said, and each call it made.
    private var conversation: some View {
        ScrollViewReader { scroll in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    if assistant.entries.isEmpty {
                        Text(introduction).foregroundStyle(.secondary)
                        if keyless {
                            Button("Add a Key…", action: openKeys)
                                .accessibilityIdentifier("add-key")
                        }
                    }
                    ForEach(assistant.entries) { entry in
                        EntryRow(entry: entry).id(entry.id)
                    }
                    if assistant.working {
                        ProgressView().controlSize(.small)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: assistant.entries.count) {
                if let last = assistant.entries.last { scroll.scrollTo(last.id, anchor: .bottom) }
            }
        }
    }

    private var introduction: String {
        if let listing { return listing }
        return """
            Ask about the deck, or ask for an edit: "this" means the state shown and what is \
            selected. Each edit shows as it is made, and Undo takes it back. Your key goes to \
            \(provider.name) alone.
            """
    }

    /// The question, and Ask or Stop.
    private var asking: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Ask about the deck", text: $question, axis: .vertical)
                .lineLimit(1...6)
                .textFieldStyle(.roundedBorder)
                .onSubmit(ask)
                .disabled(assistant.working)
            if assistant.working {
                Button("Stop") { assistant.stop() }
                    .keyboardShortcut(".", modifiers: .command)
            } else {
                Button("Ask", action: ask)
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(question.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.wrappedValue.isEmpty)
            }
        }
        .padding(8)
    }

    private func ask() {
        let text = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !model.wrappedValue.isEmpty else { return }
        guard let key = keychain.key(for: provider) else {
            listing = noKey
            keyless = true
            return
        }
        assistant.ask(text, provider: provider, model: model.wrappedValue, key: key, seeing: seeing())
        question = ""
    }

    /// Where the keys are kept: Settings on the Mac, a sheet on the iPad (PLAN 4.9).
    private func openKeys() {
        #if os(macOS)
        openSettings()
        #else
        keying = true
        #endif
    }

    /// What the panel says while the provider has no key kept.
    private var noKey: String {
        #if os(macOS)
        return "Add your \(provider.name) key in Settings (⌘,) to ask \(provider.name)'s models."
        #else
        return "Add your \(provider.name) key to ask \(provider.name)'s models."
        #endif
    }

    /// The models the provider's key can use.
    private func list() async {
        models = []
        guard let key = keychain.key(for: provider) else {
            listing = noKey
            keyless = true
            return
        }
        keyless = false
        listing = nil
        do {
            let request = try provider.modelsRequest(key: key).urlRequest()
            let (data, status) = try await Assistant.urlSession(request)
            models = try provider.models(status: status, body: String(decoding: data, as: UTF8.self))
            if model.wrappedValue.isEmpty, let first = models.first { model.wrappedValue = first }
        } catch {
            listing = "\(provider.name) listed no models: \(error)"
        }
    }
}

/// One entry of the conversation.
private struct EntryRow: View {
    let entry: Assistant.Entry

    var body: some View {
        switch entry.kind {
        case .question(let text):
            Text(text)
                .padding(8)
                .background(.tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .textSelection(.enabled)
        case .said(let text):
            Text(markdown(text)).textSelection(.enabled)
        case .call(let name, _, let ran):
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: ran == nil ? "gearshape" : ran?.error == true ? "xmark.circle" : "checkmark.circle")
                        .foregroundStyle(ran?.error == true ? Color.red : Color.secondary)
                    Text(name).font(.callout.monospaced())
                    if let summary = ran?.summary {
                        Text(summary).font(.callout).foregroundStyle(.secondary).lineLimit(2)
                    }
                }
                if let frame = ran?.frame, let image = Image(png: frame) {
                    image
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(maxWidth: 260)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                }
            }
        case .done(let why):
            if let said = stopped(why) {
                Text(said).font(.caption).foregroundStyle(.secondary)
            }
        case .failed(let why):
            Label(why, systemImage: "exclamationmark.triangle").foregroundStyle(.orange).textSelection(.enabled)
        }
    }

    /// The model's words, their inline Markdown shown.
    private func markdown(_ text: String) -> AttributedString {
        let inline = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        return (try? AttributedString(markdown: text, options: inline)) ?? AttributedString(text)
    }

    /// Why the answer stopped, where it is worth saying.
    private func stopped(_ why: String) -> String? {
        switch why {
        case "end", "tools": nil
        case "stopped": "Stopped."
        case "steps": "Stopped after 32 rounds of calls: ask again to go on."
        case "length": "The answer ran out of room."
        default: "Stopped: \(why)."
        }
    }
}

/// The keys, in Settings: one for each provider, kept in the Keychain and sent to their provider
/// alone (ADR-0006).
struct KeysSettings: View {
    private let keychain = Keychain()
    @State private var typed: [Provider: String] = [:]
    @State private var kept: Set<Provider> = []
    @State private var failure: String?

    var body: some View {
        Form {
            Section {
                ForEach(Provider.allCases) { provider in
                    HStack {
                        #if !os(macOS)
                        // A form on the iPad shows a field's prompt, not its name.
                        Text(provider.name).frame(minWidth: 84, alignment: .leading)
                        #endif
                        SecureField(
                            provider.name,
                            text: Binding(get: { typed[provider] ?? "" }, set: { typed[provider] = $0 }),
                            prompt: Text(kept.contains(provider) ? "Kept in the Keychain" : "Paste a key")
                        )
                        .accessibilityIdentifier("key-\(provider.rawValue)")
                        Button("Keep") { keep(typed[provider], for: provider) }
                            .disabled((typed[provider] ?? "").isEmpty)
                            .accessibilityIdentifier("keep-\(provider.rawValue)")
                        Button("Remove") { keep(nil, for: provider) }
                            .disabled(!kept.contains(provider))
                            .accessibilityIdentifier("remove-\(provider.rawValue)")
                    }
                    #if !os(macOS)
                    // A row of a form on the iPad takes a tap as every button's in it, Keep's and
                    // Remove's at once, unless each is borderless.
                    .buttonStyle(.borderless)
                    #endif
                }
            } header: {
                Text("The assistant's keys")
            } footer: {
                Text(
                    "Each key stays in your Keychain and goes only to its provider. It is never written into a deck."
                )
                .foregroundStyle(.secondary)
            }
            if let failure {
                Text(failure).foregroundStyle(.red)
            }
        }
        .formStyle(.grouped)
        #if os(macOS)
        .frame(width: 480)
        #else
        .navigationTitle("Keys")
        #endif
        .onAppear { kept = Set(keychain.providers) }
    }

    private func keep(_ key: String?, for provider: Provider) {
        do {
            try keychain.set(key?.trimmingCharacters(in: .whitespacesAndNewlines), for: provider)
            typed[provider] = nil
            kept = Set(keychain.providers)
            failure = nil
        } catch {
            failure = "\(error)"
        }
    }
}
