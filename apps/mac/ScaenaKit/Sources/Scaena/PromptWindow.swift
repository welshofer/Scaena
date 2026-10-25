// New from a Prompt (PLAN G.6): on the Mac, a window File › New from a Prompt… opens; on the iPad,
// the brief shown over the document browser, from its launch screen, since iPadOS makes a new
// document only there.
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers
#if !os(macOS)
import UIKit
#endif

#if os(macOS)
/// The window File › New from a Prompt… opens (PLAN G.6).
enum PromptWindow {
    static let id = "prompt"
}

/// File › New from a Prompt… (⌥⌘N), beside New on the Mac's menu bar.
struct PromptCommands: Commands {
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Button("New from a Prompt…") { openWindow(id: PromptWindow.id) }
                .keyboardShortcut("n", modifiers: [.option, .command])
        }
    }
}
#else
/// New from a Prompt on the iPad (PLAN G.6): the document browser's launch screen offers it beside
/// New Deck, and its brief covers the screen (`ScaenaApp`). The deck made is kept in the app's own
/// folder, On My iPad › Scaena, and opened as the browser opens a deck a person picks there.
@MainActor
enum Prompting {
    /// `document` written as a bundle named for what it is about in the app's own folder, numbered
    /// where a deck there has the name already; the deck to say `said` as it opens.
    static func written(_ document: ScaenaDocument, about topic: String, said: String) throws -> URL {
        let name = fileName(topic)
        let folder = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        var bundle = folder.appending(path: "\(name).scaena", directoryHint: .isDirectory)
        var n = 2
        while FileManager.default.fileExists(atPath: bundle.path) {
            bundle = folder.appending(path: "\(name) \(n).scaena", directoryHint: .isDirectory)
            n += 1
        }
        let files = try document.savedNow()
        try ScaenaDocument.wrapper(files).write(to: bundle, options: .atomic, originalContentsURL: nil)
        ScaenaDocument.handing(said, as: name)
        return bundle
    }

    /// The deck at `bundle` opened as the document browser opens one a person picks: shown in the
    /// browser, then handed to it as picked. Each step is logged, the iPad's UI tests reading it.
    static func open(_ bundle: URL) {
        guard let browser = browser() else {
            openingLog.debug("New from a Prompt: no document browser to open the deck in")
            return
        }
        browser.revealDocument(at: bundle, importIfNeeded: false) { shown, error in
            Task { @MainActor in
                guard let shown else {
                    openingLog.debug("New from a Prompt: the deck not shown: \(String(describing: error), privacy: .public)")
                    return
                }
                browser.delegate?.documentBrowser?(browser, didPickDocumentsAt: [shown])
                openingLog.debug("New from a Prompt: the deck handed to the browser")
            }
        }
    }

    /// The document browser in a window of the app, the launch screen's: found among each window's
    /// view controllers, what each holds and what each presents.
    private static func browser() -> UIDocumentBrowserViewController? {
        func found(in controller: UIViewController?) -> UIDocumentBrowserViewController? {
            guard let controller else { return nil }
            if let browser = controller as? UIDocumentBrowserViewController { return browser }
            for child in controller.children {
                if let browser = found(in: child) { return browser }
            }
            return found(in: controller.presentedViewController)
        }
        let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap(\.windows)
        for window in windows.filter(\.isKeyWindow) + windows.filter({ !$0.isKeyWindow }) {
            if let browser = found(in: window.rootViewController) { return browser }
        }
        return nil
    }

    /// A deck's file name from what it is about: its first line, without a slash or a colon, at
    /// most 60 characters; Untitled where that leaves nothing.
    static func fileName(_ topic: String) -> String {
        var name = (topic.split(whereSeparator: \.isNewline).first.map(String.init) ?? "")
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ":", with: "-")
            .trimmingCharacters(in: .whitespaces)
        while name.hasPrefix(".") { name.removeFirst() }
        name = String(name.prefix(60)).trimmingCharacters(in: .whitespaces)
        return name.isEmpty ? "Untitled" : name
    }
}
#endif

/// New from a Prompt (PLAN G.6, ADR-0026): a deck written by Narrato from what the user asks,
/// grounded in the files and links they give, by the provider and model they choose with the key
/// the Keychain keeps, its pictures made by their image model, as the browser's From a prompt… and
/// `scaena generate` make one. The engine reads each source, builds each request, and makes the
/// deck a bundle (`Generator`); each request is made here by `URLSession`. On the Mac the deck
/// opens in a window of its own, kept nowhere until it is saved; on the iPad it is handed to the
/// document browser, which keeps it and opens it (`Prompting`).
struct PromptView: View {
    #if os(macOS)
    @Environment(\.newDocument) private var newDocument
    @Environment(\.dismissWindow) private var dismissWindow
    /// Whether the window is the one in front: the keys are read again as it comes back from
    /// Settings, where they are kept on the Mac.
    @Environment(\.appearsActive) private var appearsActive
    @Environment(\.openSettings) private var openSettings
    #else
    /// What becomes of the deck: its bundle handed to the document browser, or none.
    private let finish: (URL?) -> Void
    /// The keys, on the iPad in a sheet, as the assistant's are (PLAN 4.9).
    @State private var keying = false
    /// Bumped as the keys sheet closes, so that the keys are read again.
    @State private var keysChanged = 0
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
    /// Whether the outline is shown first (narrative FUNC-1): the arc, the big idea, and each beat's
    /// claim, settled before the slides are written.
    @AppStorage("generate.outline") private var showOutline = true
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
    @State private var steps: [Line] = []
    /// Each slide as drawn for the model, JPEGs in the deck's order (lift-up ATTR-1).
    @State private var film: [Data] = []
    @State private var running: Task<Void, Never>?
    /// Cancel, or Escape, pressed while a deck is being made: asked first (lift-up 10-24 USE-2).
    @State private var confirmingStop = false
    /// The outline shown to settle (narrative FUNC-1), its beats as the person edits them, and the
    /// generation waiting on what they decide.
    @State private var outlined: Outlined?
    @State private var bigIdea = ""
    @State private var beats: [EditedBeat] = []
    @State private var outlineArc = ""
    @State private var deciding: CheckedContinuation<OutlineDecision?, Never>?

    /// What a brief may ask, read once.
    private static let offered = try? Generator.choices()
    private var choices: BriefChoices? { Self.offered }
    private let keychain = Keychain()

    #if os(macOS)
    init() {}
    #else
    /// The brief over the document browser: `finish` takes the bundle of the deck made, or none.
    init(finish: @escaping (URL?) -> Void) {
        self.finish = finish
    }
    #endif

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

    /// A beat of the outline as the person edits it: moved, removed, or added, its claim typed over.
    struct EditedBeat: Identifiable {
        let id = UUID()
        var name: String
        var claim: String
        var slides: Int

        init(_ beat: StoryOutline.Beat) {
            name = beat.name
            claim = beat.claim
            slides = beat.slides
        }

        init(name: String = "", claim: String = "", slides: Int = 0) {
            self.name = name
            self.claim = claim
            self.slides = slides
        }

        var beat: StoryOutline.Beat { .init(name: name, claim: claim, slides: slides) }

        /// The slides it takes, said.
        var takes: String { slides == 0 ? "no slides" : slides == 1 ? "1 slide" : "\(slides) slides" }
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
                    field("Audience", text: $audience, prompt: choices?.defaults.audience ?? "Colleagues")
                    Picker("Goal", selection: $goal) {
                        ForEach(choices?.goals ?? [], id: \.self) { Text($0).tag($0) }
                    }
                    Picker("Length", selection: $length) {
                        ForEach(choices?.lengths ?? [], id: \.self) { Text(Self.said(length: $0)).tag($0) }
                    }
                    .accessibilityIdentifier("prompt-length")
                    // The themes that ship, by family, each family a section (PLAN T.0d).
                    Picker("Theme", selection: $theme) {
                        ForEach((choices?.themes ?? []).byFamily) { family in
                            Section(family.name) {
                                ForEach(family.themes) { ships in
                                    Text(Words.phrase(ships.name)).tag(ships.name)
                                }
                            }
                        }
                    }
                }
                Section {
                    Picker("Provider", selection: $provider) {
                        ForEach(Provider.allCases) { Text($0.name).tag($0) }
                    }
                    if !keyed {
                        Button("Add \(article(provider.name)) Key…", action: openKeys)
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
                } header: {
                    Text("Written by")
                } footer: {
                    #if !os(macOS)
                    // Under the section it is about, in line with the form, on the iPad.
                    Text(who)
                    #endif
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
                        // What a link may be, in a person's words, with how a company's filings are
                        // asked for (`scaena_sources::fetch::remote` reads each).
                        TextField(
                            "Link", text: $link, prompt: Text("A web address, or a company’s filings, as sec:AAPL"))
                            .onSubmit(addLink)
                            .accessibilityIdentifier("prompt-link")
                        Button("Add Link", action: addLink)
                            .buttonStyle(.borderless)
                            .disabled(link.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                    Toggle("Research the topic first, for some minutes", isOn: $research)
                        .disabled(provider == .openai)
                }
                Section {
                    Toggle("Show the outline first", isOn: $showOutline)
                        .accessibilityIdentifier("prompt-show-outline")
                } footer: {
                    Text("The arc, the big idea, and what each beat says, to change before the slides are written.")
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
                        field("Language", text: $language, prompt: "English")
                        field("Voice", text: $voice, prompt: "Plain, confident, no jargon")
                        field("Your email, for SEC", text: $secContact, prompt: "SEC asks each request to name who makes it")
                        field("GitHub token", text: $token, prompt: "For a private repository", secure: true)
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(running != nil)
            #if os(macOS)
            Text(who).font(.caption).foregroundStyle(.secondary).padding(.horizontal)
            #endif
            if outlined != nil {
                outlineView
            }
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
            if !film.isEmpty {
                filmstrip
            }
            #if os(macOS)
            HStack {
                Spacer()
                cancelling
                generating
            }
            .padding([.horizontal, .bottom])
            #endif
        }
        #if os(macOS)
        .frame(minWidth: 520, idealWidth: 600, minHeight: 560, idealHeight: 720)
        #else
        // Cancel at the bar's start and Generate at its end, as an iPad's full-screen sheet has
        // them.
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { cancelling }
            ToolbarItem(placement: .confirmationAction) { generating }
        }
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .navigationTitle("New from a Prompt")
        .fileImporter(
            isPresented: $importing,
            allowedContentTypes: [.pdf, .plainText, .commaSeparatedText, .html, .json, .rtf, .spreadsheet, .presentation, .data],
            allowsMultipleSelection: true
        ) { picked in
            if case .success(let urls) = picked { given += urls.map(Given.file) }
        }
        #if os(macOS)
        .task(id: appearsActive) { readKeys() }
        #else
        .task(id: keysChanged) { readKeys() }
        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
            KeysSheet { keying = false }
        }
        #endif
        .task(id: "\(provider.rawValue)\u{1f}\(keyed)") { await list() }
        .onAppear {
            if goal.isEmpty { goal = choices?.defaults.goal ?? "" }
            if length.isEmpty { length = choices?.defaults.length ?? "" }
            if modality.isEmpty { modality = choices?.modalities.first?.id ?? "" }
            // A theme kept by the name it shipped under before: Gallery is Salon (PLAN T.0d).
            if theme == "gallery" { theme = "salon" }
            // Launched with `-sampleOutline YES`, the window shows an outline as one reads, with no
            // call made: what the screenshots CI keeps show of the step.
            if UserDefaults.standard.bool(forKey: "sampleOutline"), outlined == nil { show(Outlined.sample) }
        }
    }

    /// The outline shown to settle (narrative FUNC-1): the arc and why, the big idea to type over,
    /// each beat with its claim, moved, removed, or added; another arc to outline on; and Write the
    /// Slides, which writes them to the outline as it stands.
    private var outlineView: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("The outline").font(.headline)
            if let outlined {
                Text(
                    outlined.fallback.map { "Arc: \(outlined.arc), the heuristic's (\($0))." }
                        ?? "Arc: \(outlined.arc). \(outlined.reasoning)"
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            TextField("Big idea", text: $bigIdea, prompt: Text("The one sentence the audience should remember"), axis: .vertical)
                .lineLimit(1...3)
                .accessibilityIdentifier("prompt-big-idea")
            List {
                ForEach($beats) { $beat in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            TextField("Beat", text: $beat.name, prompt: Text("The beat's name"))
                                .font(.caption.weight(.semibold))
                                .textFieldStyle(.plain)
                                .accessibilityIdentifier("prompt-beat-name")
                            Spacer()
                            Text(beat.takes).font(.caption).foregroundStyle(.secondary)
                            Button("Remove") { beats.removeAll { $0.id == beat.id } }
                                .buttonStyle(.borderless)
                                .font(.caption)
                                .accessibilityLabel("Remove the beat")
                        }
                        TextField("Claim", text: $beat.claim, prompt: Text("What its slides leave the audience believing"), axis: .vertical)
                            .lineLimit(1...3)
                            .accessibilityIdentifier("prompt-beat-claim")
                    }
                }
                .onMove { from, to in beats.move(fromOffsets: from, toOffset: to) }
                .onDelete { at in beats.remove(atOffsets: at) }
            }
            .frame(minHeight: 160, idealHeight: 260)
            .accessibilityIdentifier("prompt-beats")
            HStack {
                Button("Add a Beat") { beats.append(EditedBeat()) }
                    .accessibilityIdentifier("prompt-add-beat")
                #if !os(macOS)
                // A beat is moved in the list's edit mode on the iPad; the Mac drags a row.
                EditButton()
                #endif
                Spacer()
                Picker("Arc", selection: $outlineArc) {
                    ForEach(choices?.arcs ?? [], id: \.self) { Text($0).tag($0) }
                }
                .frame(maxWidth: 260)
                Button("Outline Again") { decide(.anotherArc(outlineArc.isEmpty ? nil : outlineArc)) }
                    .accessibilityIdentifier("prompt-outline-again")
                Button("Write the Slides") {
                    let settled = StoryOutline(bigIdea: bigIdea, beats: beats.map(\.beat))
                    guard settled.beats.contains(where: { !$0.claim.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) else {
                        return say("Keep at least one beat with a claim.", failed: true)
                    }
                    decide(.settled(settled))
                }
                .accessibilityIdentifier("prompt-write")
            }
        }
        .padding(.horizontal)
        // A container of its own, so its fields and buttons keep their own identifiers.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("prompt-outline")
    }

    /// `outlined` shown, its beats to edit.
    private func show(_ outlined: Outlined) {
        self.outlined = outlined
        bigIdea = outlined.outline.bigIdea
        beats = outlined.outline.beats.map { EditedBeat($0) }
        outlineArc = outlined.arc
    }

    /// What the person decided of the outline: the generation goes on from it, the outline put away.
    private func decide(_ decision: OutlineDecision?) {
        outlined = nil
        deciding?.resume(returning: decision)
        deciding = nil
    }

    /// Cancel: what is under way stopped, and the brief put away. While a deck is being made, it
    /// asks first: stopping drops what has been made so far, and the calls already paid for.
    private var cancelling: some View {
        Button("Cancel") {
            if running == nil { stop() } else { confirmingStop = true }
        }
        .keyboardShortcut(.cancelAction)
        .accessibilityIdentifier("prompt-cancel")
        .confirmationDialog("Stop making the deck?", isPresented: $confirmingStop) {
            Button("Stop Making It", role: .destructive) { stop() }
                .accessibilityIdentifier("prompt-stop")
            Button("Keep Making It", role: .cancel) {}
        } message: {
            Text("What has been made so far is lost.")
        }
    }

    /// What is under way stopped, and the brief put away.
    private func stop() {
        decide(nil)
        running?.cancel()
        running = nil
        #if os(macOS)
        dismissWindow(id: PromptWindow.id)
        #else
        finish(nil)
        #endif
    }

    /// Generate: the deck written, once it has something to be about and a key to write it with.
    private var generating: some View {
        Button(running == nil ? "Generate" : "Generating…", action: generate)
            .keyboardShortcut(.defaultAction)
            .disabled(!ready)
            .accessibilityIdentifier("prompt-generate")
    }

    /// A field with its label beside it, and what it asks for in it: on the iPad, a form shows a
    /// field's prompt alone, and one that is an example read as filled in. There the label is its
    /// own words at the row's start, which VoiceOver reads apart from the field; the field takes
    /// the rest of the row, its words at its end, as a settings row has them.
    @ViewBuilder private func field(_ label: String, text: Binding<String>, prompt: String, secure: Bool = false) -> some View {
        #if os(macOS)
        if secure {
            SecureField(label, text: text, prompt: Text(prompt))
        } else {
            TextField(label, text: text, prompt: Text(prompt))
        }
        #else
        HStack {
            Text(label)
            Group {
                if secure {
                    SecureField(label, text: text, prompt: Text(prompt))
                } else {
                    TextField(label, text: text, prompt: Text(prompt))
                }
            }
            .multilineTextAlignment(.trailing)
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
            return "Narrato writes the deck with your own key: keep \(article(provider.name)) key and choose a model first."
        }
        return "Written by \(provider.name)’s \(model.wrappedValue), with your key, which goes to \(provider.name) alone."
    }

    /// Who makes the pictures, or why none will be made.
    private var paintedBy: String {
        guard let made = Provider(rawValue: pictures) else { return "" }
        guard painter != nil else {
            return "Keep \(article(made.name)) key to have the slides’ pictures made, or choose None."
        }
        return "Made by \(made.name)’s best image model your key lists, two in five slides, ten at most. Your key goes to \(made.name) alone."
    }

    /// A length as the brief shows it, its range with an en dash: `5–10 slides`. What is chosen
    /// stays Narrato's own words, `5-10 slides`, which its prompts carry byte for byte (ADR-0026).
    private static func said(length: String) -> String {
        length.replacingOccurrences(of: "-", with: "\u{2013}")
    }

    /// `name` after its article: "an Anthropic", "an OpenAI", "a Gemini".
    private func article(_ name: String) -> String {
        "AEIOU".contains(name.first ?? "x") ? "an \(name)" : "a \(name)"
    }

    /// Where the keys are kept: Settings on the Mac (PLAN 3.6), a sheet on the iPad (PLAN 4.9).
    private func openKeys() {
        #if os(macOS)
        openSettings()
        #else
        keying = true
        #endif
    }

    /// The providers whose keys the Keychain keeps, read again.
    private func readKeys() {
        kept = Set(Provider.allCases.filter { keychain.key(for: $0) != nil })
    }

    /// The models the provider's key can use, the one kept chosen, else the first; where it lists
    /// none, why, in a person's words (PLAN 3.18), as the assistant's panel says it.
    private func list() async {
        models = []
        listing = nil
        guard let key = keychain.key(for: provider) else { return }
        let listed = await KeyCheck.models(key, for: provider)
        // Another provider chosen, or a key kept, since: the listing that follows says.
        guard !Task.isCancelled else { return }
        switch listed {
        case .listed(let names):
            models = names
            if !models.contains(model.wrappedValue), let first = models.first { model.wrappedValue = first }
        case .unlisted(let why):
            listing = why
        }
    }

    private func addLink() {
        let l = link.trimmingCharacters(in: .whitespaces)
        guard !l.isEmpty else { return }
        given.append(.link(l))
        link = ""
    }

    /// The slides as drawn for the model, in a strip under the progress, so the wait shows the
    /// deck coming together (lift-up ATTR-1).
    private var filmstrip: some View {
        ScrollView(.horizontal) {
            LazyHStack(spacing: 6) {
                ForEach(film.indices, id: \.self) { i in
                    Group {
                        if let picture = Image(png: film[i]) {
                            picture.resizable().aspectRatio(contentMode: .fit)
                        } else {
                            Color.secondary.opacity(0.2)
                        }
                    }
                    .frame(height: 72)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                    .accessibilityElement()
                    .accessibilityLabel("Slide \(i + 1)")
                    .accessibilityAddTraits(.isImage)
                    .transition(.opacity)
                }
            }
        }
        .frame(height: 76)
        .padding(.horizontal)
        .accessibilityIdentifier("prompt-film")
    }

    private func say(_ words: String, failed: Bool = false) {
        steps.append(Line(words: words, failed: failed))
    }

    private func generate() {
        guard ready, let key = keychain.key(for: provider) else { return }
        steps = []
        film = []
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
        // The outline shown to settle (narrative FUNC-1), the generation waiting on what is decided
        // of it; none where the slides are to come at once.
        let settle: Generator.Settle?
        if showOutline {
            settle = { @MainActor (outlined: Outlined) async -> OutlineDecision? in
                await withCheckedContinuation { (continuation: CheckedContinuation<OutlineDecision?, Never>) in
                    show(outlined)
                    deciding = continuation
                }
            }
        } else {
            settle = nil
        }
        running = Task { @MainActor in
            defer { running = nil }
            #if !os(macOS)
            // Run on if the person goes to another app meanwhile, as long as iPadOS allows, and say
            // why the deck was not made where it allows no longer (lift-up REL-5).
            let kept = KeptRunning("Making a deck") {
                running?.cancel()
                say(KeptRunning.stopped, failed: true)
            }
            defer { kept.end() }
            #endif
            let generator = Generator()
            do {
                // Before a file is read or a call is paid for: the font for the script the deck
                // is written in, where the theme's fonts do not draw it, and the theme asked
                // whether its fonts then draw the language (PLAN G.21, G.22).
                try await generator.prepare(language: brief.language, theme: theme) { say($0.words) }
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
                    theme: theme, painter: paint, outline: settle,
                    drawn: { slide, jpeg in
                        withAnimation {
                            if film.indices.contains(slide) { film[slide] = jpeg } else { film.append(jpeg) }
                        }
                    }
                ) { say($0.words) }
                try Task.checkCancellation()
                #if os(macOS)
                // A reference document is made by a closure, not an autoclosure.
                newDocument { ScaenaDocument(generated: session, said: made.words) }
                dismissWindow(id: PromptWindow.id)
                #else
                let document = ScaenaDocument(generated: session, said: made.words)
                finish(try Prompting.written(document, about: brief.topic, said: made.words))
                #endif
            } catch is CancellationError {
            } catch where Task.isCancelled {
                // Stopped (by Cancel, or by iPadOS, which said so): a request cut off says nothing more.
            } catch {
                // In a sentence, without the error's wrapping (PLAN 3.18). What the provider said
                // of the brief stays its own (the generator's tests hold it so).
                say(Why.said(error), failed: true)
            }
        }
    }
}
