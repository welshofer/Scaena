import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

/// The window File › New from a Prompt… opens (PLAN G.6).
enum PromptWindow {
    static let id = "prompt"
}

/// File › New from a Prompt… (⌥⌘N), beside New: on the Mac's menu bar and the iPad's.
struct PromptCommands: Commands {
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Button("New from a Prompt…") { openWindow(id: PromptWindow.id) }
                .keyboardShortcut("n", modifiers: [.option, .command])
        }
    }
}

/// New from a Prompt (PLAN G.6, ADR-0026): a deck written by Narrato from what the user asks,
/// grounded in the files and links they give, by the provider and model they choose with the key
/// the Keychain keeps, its pictures made by their image model, as the browser's From a prompt… and
/// `scaena generate` make one. The engine reads each source, builds each request, and makes the
/// deck a bundle (`Generator`); each request is made here by `URLSession`. The deck opens in a
/// window of its own, kept nowhere until it is saved.
struct PromptView: View {
    @Environment(\.newDocument) private var newDocument
    @Environment(\.dismissWindow) private var dismissWindow
    /// Whether the window is the one in front: the keys are read again as it comes back from
    /// Settings, where they are kept on the Mac.
    @Environment(\.appearsActive) private var appearsActive
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #endif
    @AppStorage("assistant.provider") private var provider = Provider.anthropic
    @AppStorage("assistant.model.anthropic") private var anthropicModel = ""
    @AppStorage("assistant.model.openai") private var openaiModel = ""
    @AppStorage("assistant.model.gemini") private var geminiModel = ""
    /// Who makes the pictures: `openai`, `gemini`, or `none`.
    @AppStorage("generate.pictures") private var pictures = "openai"
    /// How they look: `illustration` or `photo`.
    @AppStorage("generate.look") private var look = "illustration"
    @AppStorage("generate.theme") private var theme = "dusk"
    /// What SEC asks each request to carry: the user's own email, kept on this device.
    @AppStorage("generate.secContact") private var secContact = ""

    @State private var topic = ""
    @State private var audience = ""
    @State private var goal = ""
    @State private var length = ""
    @State private var modality = ""
    @State private var arc = ""
    @State private var outcome = ""
    @State private var language = ""
    @State private var voice = ""
    @State private var token = ""
    @State private var research = false
    @State private var given: [Given] = []
    @State private var link = ""
    @State private var importing = false
    @State private var models: [String] = []
    @State private var listing: String?
    /// The providers whose keys the Keychain keeps.
    @State private var kept: Set<Provider> = []
    /// Bumped as the keys sheet closes, so that the keys are read again.
    @State private var keysChanged = 0
    #if !os(macOS)
    /// The keys, on the iPad in a sheet: it has no Settings window (PLAN 4.2).
    @State private var keying = false
    #endif
    @State private var steps: [Line] = []
    @State private var running: Task<Void, Never>?

    /// What a brief may ask, read once.
    private static let offered = try? Generator.choices()
    private var choices: BriefChoices? { Self.offered }
    private let keychain = Keychain()

    /// A source given: a file, or a link.
    enum Given: Hashable, Identifiable {
        case file(URL)
        case link(String)

        var id: String {
            switch self {
            case .file(let url): url.absoluteString
            case .link(let link): link
            }
        }

        var name: String {
            switch self {
            case .file(let url): url.lastPathComponent
            case .link(let link): link
            }
        }
    }

    /// A line of how far it has come.
    struct Line: Identifiable {
        let id = UUID()
        let words: String
        let failed: Bool
    }

    /// The model chosen for the provider, as the assistant keeps it.
    private var model: Binding<String> {
        switch provider {
        case .anthropic: $anthropicModel
        case .openai: $openaiModel
        case .gemini: $geminiModel
        }
    }

    /// Whether a key for the provider is kept.
    private var keyed: Bool { kept.contains(provider) }

    /// Who makes the pictures, where a key for them is kept.
    private var painter: Provider? {
        guard let made = Provider(rawValue: pictures), made != .anthropic, kept.contains(made) else { return nil }
        return made
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Form {
                Section {
                    TextField("What it is about", text: $topic, prompt: Text("A deck about…"), axis: .vertical)
                        .lineLimit(2...5)
                        .accessibilityIdentifier("prompt-topic")
                    TextField("Audience", text: $audience, prompt: Text(choices?.defaults.audience ?? "Colleagues"))
                    Picker("Goal", selection: $goal) {
                        ForEach(choices?.goals ?? [], id: \.self) { Text($0).tag($0) }
                    }
                    Picker("Length", selection: $length) {
                        ForEach(choices?.lengths ?? [], id: \.self) { Text($0).tag($0) }
                    }
                    Picker("Theme", selection: $theme) {
                        Text("Dusk").tag("dusk")
                        Text("Daybreak").tag("daybreak")
                        Text("Ember").tag("ember")
                    }
                }
                Section("Written by") {
                    Picker("Provider", selection: $provider) {
                        ForEach(Provider.allCases) { Text($0.name).tag($0) }
                    }
                    if !keyed {
                        Button("Add a \(provider.name) Key…", action: openKeys)
                    } else {
                        Picker("Model", selection: model) {
                            if !models.contains(model.wrappedValue) {
                                Text(model.wrappedValue.isEmpty ? "Choose a model" : model.wrappedValue)
                                    .tag(model.wrappedValue)
                            }
                            ForEach(models, id: \.self) { Text($0).tag($0) }
                        }
                        .accessibilityIdentifier("prompt-model")
                    }
                    if let listing {
                        Text(listing).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Section("Pictures") {
                    Picker("Made by", selection: $pictures) {
                        Text("OpenAI").tag("openai")
                        Text("Gemini").tag("gemini")
                        Text("None").tag("none")
                    }
                    if pictures != "none" {
                        Picker("Look", selection: $look) {
                            Text("Illustrations").tag("illustration")
                            Text("Photos").tag("photo")
                        }
                        Text(paintedBy).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Section("Grounded in") {
                    ForEach(given) { source in
                        HStack {
                            Text(source.name)
                            Spacer()
                            Button("Remove") { given.removeAll { $0 == source } }
                                .buttonStyle(.borderless)
                        }
                    }
                    Button("Add Files…") { importing = true }
                        .buttonStyle(.borderless)
                    HStack {
                        TextField("Link", text: $link, prompt: Text("sec:TICKER, github:owner/repo, or a web address"))
                            .onSubmit(addLink)
                        Button("Add Link", action: addLink)
                            .buttonStyle(.borderless)
                            .disabled(link.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                    Toggle("Research the topic first, for some minutes", isOn: $research)
                        .disabled(provider == .openai)
                }
                Section {
                    DisclosureGroup("More") {
                        Picker("Delivered", selection: $modality) {
                            ForEach(choices?.modalities ?? [], id: \.id) { Text($0.label).tag($0.id) }
                        }
                        Picker("Arc", selection: $arc) {
                            Text("The model’s choice").tag("")
                            ForEach(choices?.arcs ?? [], id: \.self) { Text($0).tag($0) }
                        }
                        TextField("What it should achieve", text: $outcome)
                        TextField("Language", text: $language, prompt: Text("English"))
                        TextField("Voice", text: $voice, prompt: Text("Plain, confident, no jargon"))
                        TextField("Your email, for SEC", text: $secContact, prompt: Text("SEC asks each request to name who makes it"))
                        SecureField("GitHub token", text: $token, prompt: Text("For a private repository"))
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(running != nil)
            Text(who).font(.caption).foregroundStyle(.secondary).padding(.horizontal)
            if !steps.isEmpty {
                ScrollViewReader { scroller in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 2) {
                            ForEach(steps) { line in
                                Text(line.words)
                                    .foregroundStyle(line.failed ? .red : .primary)
                                    .font(.callout)
                                    .id(line.id)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(maxHeight: 120)
                    .onChange(of: steps.count) {
                        if let last = steps.last { scroller.scrollTo(last.id, anchor: .bottom) }
                    }
                }
                .padding(.horizontal)
                .accessibilityIdentifier("prompt-progress")
            }
            HStack {
                Spacer()
                Button("Cancel") {
                    running?.cancel()
                    running = nil
                    dismissWindow(id: PromptWindow.id)
                }
                .keyboardShortcut(.cancelAction)
                Button(running == nil ? "Generate" : "Generating…", action: generate)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ready)
                    .accessibilityIdentifier("prompt-generate")
            }
            .padding([.horizontal, .bottom])
        }
        .frame(minWidth: 520, idealWidth: 600, minHeight: 560, idealHeight: 720)
        .navigationTitle("New from a Prompt")
        .fileImporter(
            isPresented: $importing,
            allowedContentTypes: [.pdf, .plainText, .commaSeparatedText, .html, .json, .rtf, .spreadsheet, .presentation, .data],
            allowsMultipleSelection: true
        ) { picked in
            if case .success(let urls) = picked { given += urls.map(Given.file) }
        }
        .task(id: "\(appearsActive)\u{1f}\(keysChanged)") {
            kept = Set(Provider.allCases.filter { keychain.key(for: $0) != nil })
        }
        .task(id: "\(provider.rawValue)\u{1f}\(keyed)") { await list() }
        .onAppear {
            if goal.isEmpty { goal = choices?.defaults.goal ?? "" }
            if length.isEmpty { length = choices?.defaults.length ?? "" }
            if modality.isEmpty { modality = choices?.modalities.first?.id ?? "" }
        }
        #if !os(macOS)
        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
            NavigationStack {
                KeysSettings()
                    .toolbar { Button("Done") { keying = false } }
            }
        }
        #endif
    }

    /// Whether Generate can be pressed: a topic, a key, a model, and nothing under way.
    private var ready: Bool {
        running == nil && keyed && !model.wrappedValue.isEmpty
            && !topic.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Who writes the deck, with whose key.
    private var who: String {
        guard keyed, !model.wrappedValue.isEmpty else {
            return "Narrato writes the deck with your own key: keep a \(provider.name) key and choose a model first."
        }
        return "Written by \(provider.name)’s \(model.wrappedValue), with your key, which goes to \(provider.name) alone."
    }

    /// Who makes the pictures, or why none will be made.
    private var paintedBy: String {
        guard let made = Provider(rawValue: pictures) else { return "" }
        guard painter != nil else {
            return "Keep a \(made.name) key to have the slides’ pictures made, or choose None."
        }
        return "Made by \(made.name)’s best image model your key lists, two in five slides, ten at most. Your key goes to \(made.name) alone."
    }

    /// Where the keys are kept: Settings on the Mac, a sheet on the iPad (PLAN 4.9).
    private func openKeys() {
        #if os(macOS)
        openSettings()
        #else
        keying = true
        #endif
    }

    /// The models the provider's key can use, the one kept chosen, else the first.
    private func list() async {
        models = []
        listing = nil
        guard let key = keychain.key(for: provider) else { return }
        do {
            let request = try provider.modelsRequest(key: key).urlRequest()
            let (data, status) = try await Assistant.urlSession(request)
            models = try provider.models(status: status, body: String(decoding: data, as: UTF8.self))
            if !models.contains(model.wrappedValue), let first = models.first { model.wrappedValue = first }
        } catch {
            listing = "\(provider.name) listed no models: \(error.localizedDescription)"
        }
    }

    private func addLink() {
        let l = link.trimmingCharacters(in: .whitespaces)
        guard !l.isEmpty else { return }
        given.append(.link(l))
        link = ""
    }

    private func say(_ words: String, failed: Bool = false) {
        steps.append(Line(words: words, failed: failed))
    }

    private func generate() {
        guard ready, let key = keychain.key(for: provider) else { return }
        steps = []
        let brief = Brief(
            topic: topic.trimmingCharacters(in: .whitespacesAndNewlines),
            audience: audience.isEmpty ? nil : audience, goal: goal.isEmpty ? nil : goal,
            length: length.isEmpty ? nil : length, modality: modality.isEmpty ? nil : modality,
            desiredOutcomes: outcome.isEmpty ? nil : outcome, language: language.isEmpty ? nil : language,
            voice: voice.isEmpty ? nil : voice)
        let paint = painter.flatMap { made in
            keychain.key(for: made).map { Generator.Painter(provider: made, key: $0, look: look) }
        }
        let (provider, model, theme, arc, given) = (
            self.provider, self.model.wrappedValue, self.theme, self.arc, self.given
        )
        let (research, contact, token) = (self.research && self.provider != .openai, self.secContact, self.token)
        running = Task { @MainActor in
            defer { running = nil }
            let generator = Generator()
            do {
                for source in given {
                    switch source {
                    case .file(let url):
                        say(GeneratingStep.reading(url.lastPathComponent).words)
                        let reading = url.startAccessingSecurityScopedResource()
                        defer { if reading { url.stopAccessingSecurityScopedResource() } }
                        say(try generator.add(name: url.lastPathComponent, data: try Data(contentsOf: url)).words)
                    case .link(let link):
                        say(GeneratingStep.reading(link).words)
                        let read = try await generator.fetch(
                            link: link, contact: contact.isEmpty ? nil : contact, token: token.isEmpty ? nil : token)
                        say(read.words)
                    }
                    try Task.checkCancellation()
                }
                if research {
                    try await generator.research(brief: brief, provider: provider, model: model, key: key) {
                        say($0.words)
                    }
                    try Task.checkCancellation()
                }
                let (session, made) = try await generator.generate(
                    brief: brief, arc: arc.isEmpty ? nil : arc, provider: provider, model: model, key: key,
                    theme: theme, painter: paint
                ) { say($0.words) }
                try Task.checkCancellation()
                newDocument(ScaenaDocument(generated: session, said: made.words))
                dismissWindow(id: PromptWindow.id)
            } catch is CancellationError {
            } catch {
                say(error.localizedDescription, failed: true)
            }
        }
    }
}
