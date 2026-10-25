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
    /// Whether the conversation is scrolled to its end, where an answer is followed as it is
    /// written (lift-up 10-25c ATTR-3).
    @State private var following = true
    /// Whether the conversation shows its end now.
    @State private var atEnd = true
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
        .onChange(of: assistant.drafted, initial: true) { _, drafted in
            // A question drafted elsewhere, put in the box to send or change.
            guard let drafted else { return }
            question = drafted
            assistant.drafted = nil
        }
        #if !os(macOS)
        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
            KeysSheet { keying = false }
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
            // A model to choose once a key is kept: with none, there is none to choose, and the
            // panel offers to add one. Its name on one line, as the provider's is.
            if !keyless {
                #if os(macOS)
                Picker("Model", selection: model) { modelChoices }
                    .labelsHidden()
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .accessibilityIdentifier("assistant-model")
                #else
                // A menu picker's name is drawn by UIKit, which no line limit reaches: in the
                // panel's column it wrapped. A menu's label is SwiftUI's own, held to one line,
                // and given the room it needs before the space beside it.
                Menu {
                    Picker("Model", selection: model) { modelChoices }
                        .pickerStyle(.inline)
                        .labelsHidden()
                } label: {
                    HStack(spacing: 4) {
                        Text(modelName)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Image(systemName: "chevron.up.chevron.down")
                            .imageScale(.small)
                    }
                }
                .layoutPriority(1)
                .accessibilityLabel("Model")
                .accessibilityValue(modelName)
                .accessibilityIdentifier("assistant-model")
                #endif
            }
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

    /// The model chosen, by its name, or that one is to be chosen.
    private var modelName: String {
        model.wrappedValue.isEmpty ? "Choose a model" : model.wrappedValue
    }

    /// The models to choose from: the one chosen first, where the key does not list it.
    @ViewBuilder private var modelChoices: some View {
        if !models.contains(model.wrappedValue) {
            Text(modelName).tag(model.wrappedValue)
        }
        ForEach(models, id: \.self) { name in
            Text(name).tag(name)
        }
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
                        EntryRow(entry: entry, assistant: assistant).id(entry.id)
                    }
                    // What it is doing while it works: the call running says so itself; a
                    // provider asked again, the wait (narrative REL-3).
                    if let waiting = assistant.waiting {
                        Text(waiting).font(.callout).foregroundStyle(.secondary)
                    } else if assistant.working, !calling {
                        Text("Thinking…").font(.callout).foregroundStyle(.secondary).modifier(Sweep())
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: assistant.entries.count) {
                if let last = assistant.entries.last { scroll.scrollTo(last.id, anchor: .bottom) }
            }
            // The answer followed as it is written (lift-up 10-25c ATTR-3), as the browser's
            // panel follows it, unless the person has scrolled up from the end to read.
            .onChange(of: assistant.spoken) {
                if following, let last = assistant.entries.last { scroll.scrollTo(last.id, anchor: .bottom) }
            }
            // Followed, or not, by where the person's own scrolling leaves it: an answer growing
            // past the end, which is no scroll of theirs, leaves it followed.
            .onScrollGeometryChange(for: Bool.self, of: { geometry in
                geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 24
            }) { _, end in
                atEnd = end
            }
            .onScrollPhaseChange { _, phase in
                if !phase.isScrolling { following = atEnd }
            }
        }
    }

    /// Whether a call the model made is running, which says what it is doing itself.
    private var calling: Bool {
        if case .call(_, _, .none) = assistant.entries.last?.kind { return true }
        return false
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

    /// The models the provider's key can use; where it lists none, why, in a person's words
    /// (PLAN 3.18), the provider stood in for in the UI tests (`KeyCheck.models`).
    private func list() async {
        models = []
        guard let key = keychain.key(for: provider) else {
            listing = noKey
            keyless = true
            return
        }
        keyless = false
        listing = nil
        let listed = await KeyCheck.models(key, for: provider)
        // Another provider chosen, or the keys closed, since: the listing that follows says.
        guard !Task.isCancelled else { return }
        switch listed {
        case .listed(let names):
            models = names
            if model.wrappedValue.isEmpty, let first = names.first { model.wrappedValue = first }
        case .unlisted(let why):
            listing = why
        }
    }
}

/// A light that sweeps across words while the assistant works on them, as a model's status line
/// does; held still where Reduce Motion is on. It draws over the words, which stay as they are set.
private struct Sweep: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var still

    @ViewBuilder func body(content: Content) -> some View {
        if still {
            content
        } else {
            TimelineView(.animation) { context in
                // Once across every 1.8 s, from before the words' start to past their end.
                let head = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.8) / 1.8 * 1.6 - 0.3
                content.overlay {
                    LinearGradient(
                        colors: [.clear, Color.primary.opacity(0.6), .clear],
                        startPoint: UnitPoint(x: head - 0.25, y: 0.5),
                        endPoint: UnitPoint(x: head + 0.25, y: 0.5)
                    )
                    .mask { content }
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
                }
            }
        }
    }
}

/// One entry of the conversation.
private struct EntryRow: View {
    let entry: Assistant.Entry
    let assistant: Assistant

    var body: some View {
        switch entry.kind {
        case .question(let text):
            Text(text)
                .padding(8)
                .background(.tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .textSelection(.enabled)
        case .said(let text):
            // The answer as it reads (narrative ATTR-4): its paragraphs, headings, lists, and
            // code as blocks, each line's inline markdown read; a link to a slide shows it. The
            // deck's states are read once for the entry, not for each run.
            let known = Set(states)
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(MarkdownBlocks.blocks(of: text).enumerated()), id: \.offset) { _, block in
                    switch block {
                    case .paragraph(let words):
                        Text(markdown(words, known))
                    case .heading(_, let words):
                        Text(markdown(words, known)).font(.headline)
                    case .list(let ordered, let items):
                        VStack(alignment: .leading, spacing: 3) {
                            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                                HStack(alignment: .firstTextBaseline, spacing: 6) {
                                    Text(ordered ? "\(numbered(items, at: index))." : "•")
                                        .foregroundStyle(.secondary)
                                        .frame(minWidth: 14, alignment: .trailing)
                                    Text(markdown(item.text, known))
                                }
                                .padding(.leading, CGFloat(item.level) * 14)
                            }
                        }
                    case .code(let code, _):
                        Text(code)
                            .font(.system(.callout, design: .monospaced))
                            .padding(6)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 4))
                    }
                }
            }
            .textSelection(.enabled)
            .environment(\.openURL, OpenURLAction { url in opened(url) })
        case .call(let name, _, let ran):
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: ran == nil ? "gearshape" : ran?.made == false ? "xmark.circle" : "checkmark.circle")
                        .foregroundStyle(ran?.error == true ? Color.red : Color.secondary)
                    // What the call does, in a person's words: no tool's name reaches the window. A
                    // call refused, only tried, or stopped is headed as not done.
                    if let ran {
                        Text(ToolWords.ran(ran)).font(.callout)
                    } else {
                        Text(ToolWords.doing(name)).font(.callout).modifier(Sweep())
                    }
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
            if let said = Assistant.stopped(why) {
                Text(said).font(.caption).foregroundStyle(.secondary)
            }
        case .failed(let why):
            Label(why, systemImage: "exclamationmark.triangle").foregroundStyle(.orange).textSelection(.enabled)
        }
    }

    /// The model's words, their inline Markdown shown. A state's id in a code span, and a link
    /// to `state:ID`, link to that slide, which the window shows (narrative ATTR-4): one of
    /// `known`, the states the deck has.
    private func markdown(_ text: String, _ known: Set<String>) -> AttributedString {
        let inline = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        var read = (try? AttributedString(markdown: text, options: inline)) ?? AttributedString(text)
        for run in read.runs {
            if let link = run.link, let state = stateOf(link, known) {
                read[run.range].link = URL(string: "scaena-state:\(state)")
            } else if run.inlinePresentationIntent == .code, known.contains(String(read[run.range].characters)) {
                read[run.range].link = URL(string: "scaena-state:\(String(read[run.range].characters))")
            }
        }
        return MarkdownBlocks.hosted(read)
    }

    /// The states the deck has, which an answer may link to.
    private var states: [String] { (try? assistant.editor.session.states()) ?? [] }

    /// The state a link names, where it names one of `known`: `state:ID`, or `#ID`.
    private func stateOf(_ url: URL, _ known: Set<String>) -> String? {
        let text = url.absoluteString
        let named = text.hasPrefix("state:") ? String(text.dropFirst("state:".count)) : text.hasPrefix("#") ? String(text.dropFirst()) : nil
        guard let named, known.contains(named) else { return nil }
        return named
    }

    /// A link followed: a slide shown, or a web address opened.
    private func opened(_ url: URL) -> OpenURLAction.Result {
        if url.scheme == "scaena-state" {
            let state = String(url.absoluteString.dropFirst("scaena-state:".count))
            if states.contains(state) { assistant.editor.shown = state }
            return .handled
        }
        return url.scheme == "http" || url.scheme == "https" ? .systemAction : .discarded
    }

    /// An ordered item's number: its place among the items at its level since the last of a
    /// shallower one.
    private func numbered(_ items: [MarkdownBlock.Item], at index: Int) -> Int {
        var n = 0
        for i in 0...index {
            if items[i].level == items[index].level { n += 1 } else if items[i].level < items[index].level { n = 0 }
        }
        return n
    }

    /// Why the answer stopped, where it is worth saying.
}
