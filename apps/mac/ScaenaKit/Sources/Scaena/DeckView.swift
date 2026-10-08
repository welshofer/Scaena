import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

/// A deck's window (PLAN 3.3–3.4): its states down the side, each drawn small; the state chosen
/// painted by the engine on Metal, a node selected on it by a click, and its cue under it, to
/// play and scrub; what lint found, each with its fix; the deck's `.scn` beside the canvas; and
/// the inspector, which edits the node selected, or the state with none; and the assistant, which
/// edits it with the user (PLAN 3.6). Every edit is a patch or a source, as in the browser, and
/// one step to undo.
struct DeckView: View {
    @ObservedObject var document: ScaenaDocument
    @Environment(\.undoManager) private var undo
    /// The state chosen in the list.
    @State private var chosen: String?
    /// The node selected on the canvas or in the layers.
    @State private var node: String?
    /// The node to select once the state chosen is shown: a finding's, gone to.
    @State private var arriving: String?
    @State private var playhead = Playhead()
    @State private var failure: String?
    /// The assistant: the conversation this window keeps.
    @State private var assistant: Assistant?
    /// What the on-device model is asked, shown in a sheet.
    @State private var asked: Asked?
    /// An export shown in Quick Look (PLAN 3.8).
    @State private var looking: URL?
    /// An export to save where the user says, its type, and the name offered.
    @State private var saving: ExportedFile?
    @State private var savingType = UTType.pdf
    @State private var savingName = ""
    @SceneStorage("assistant") private var showsAssistant = false
    @SceneStorage("source") private var showsSource = false
    @SceneStorage("findings") private var showsFindings = true
    @SceneStorage("inspector") private var showsInspector = true

    private var editor: DeckEditor { document.editor }

    /// The state shown: the one chosen, or the deck's first.
    private var shown: String? {
        let slots = editor.slots
        if let chosen, slots.contains(where: { $0.state == chosen }) { return chosen }
        return slots.first?.state
    }

    // The window's parts, each its own expression: one modifier chain over all of them is more
    // than the compiler type-checks in reasonable time.
    var body: some View {
        presented(watched(window))
    }

    /// The states down the side; beside them the canvas and the rest; and the toolbar.
    private var window: some View {
        NavigationSplitView {
            StateList(editor: editor, chosen: $chosen, make: make)
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
        } detail: {
            detail
        }
        .toolbar { toolbar }
    }

    /// The source, the canvas with its cue and the findings, the assistant, and the inspector.
    private var detail: some View {
        HSplitView {
            if showsSource {
                SourcePane(text: editor.source) { typed in document.type(typed, undo: undo) }
                    .frame(minWidth: 280, idealWidth: 420)
            }
            VSplitView {
                VStack(spacing: 0) {
                    stage
                    if let shown {
                        Divider()
                        CueBar(editor: editor, state: shown, playhead: $playhead)
                    }
                }
                .frame(minHeight: 240)
                if showsFindings {
                    FindingsPanel(editor: editor, go: go, explain: OnDevice.available ? explain : nil) { finding in
                        perform { try document.fix(finding, undo: undo) }
                    }
                    .frame(minHeight: 90, idealHeight: 160)
                }
            }
            .frame(minWidth: 360)
            if showsAssistant, let assistant {
                AssistantPanel(assistant: assistant, seeing: seeing)
                    .frame(minWidth: 280, idealWidth: 340)
            }
        }
        .inspector(isPresented: $showsInspector) {
            Inspector(editor: editor, state: shown, node: $node, offer: { asked = $0 }) { ops in
                perform { try document.make(ops, undo: undo) }
            }
            .inspectorColumnWidth(min: 240, ideal: 300, max: 420)
        }
    }

    /// Play, Export, and the panes the window shows.
    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Button {
                Presenting.play(editor, from: shown)
            } label: {
                Label("Play", systemImage: "play.rectangle.fill")
            }
            .keyboardShortcut("p", modifiers: [.option, .command])
            .help("Play the deck from the state shown, on the external display if there is one (⌥⌘P)")
            .disabled(editor.slots.isEmpty)
        }
        ToolbarItem {
            Menu {
                Button("Share the PDF…") { export(.pdf, sharing: true) }
                Button("Quick Look the PDF") { export(.pdf, sharing: false) }
                Button("Save the PDF…") { save(.pdf) }
                if let shown {
                    Divider()
                    Button("Share \(shown) as a PNG…") { export(png(shown), sharing: true) }
                    Button("Save \(shown) as a PNG…") { save(png(shown)) }
                }
            } label: {
                Label("Export", systemImage: "square.and.arrow.up")
            }
            .help("The deck as a PDF, or the state shown as a PNG: shared, looked at, or saved")
            .disabled(editor.slots.isEmpty)
        }
        ToolbarItemGroup {
            Toggle(isOn: $showsSource) {
                Label("Source", systemImage: "chevron.left.forwardslash.chevron.right")
            }
            .help("The deck's .scn beside the canvas")
            Toggle(isOn: $showsFindings) {
                Label("Findings", systemImage: "exclamationmark.triangle")
            }
            .help("What lint found")
            Toggle(isOn: $showsInspector) {
                Label("Inspector", systemImage: "sidebar.trailing")
            }
            .help("The node selected, or the state")
            Toggle(isOn: $showsAssistant) {
                Label("Assistant", systemImage: "bubble.left.and.text.bubble.right")
            }
            .keyboardShortcut("a", modifiers: [.option, .command])
            .help("Ask about the deck, or for an edit, with your own key (⌥⌘A)")
        }
    }

    /// `view`, keeping up with the window: the state shown, the undo the assistant's edits go
    /// into, and every state linted once edits stop.
    private func watched(_ view: some View) -> some View {
        view
            .onChange(of: shown, initial: true) { _, now in
                // A state chosen plays its cue, as it does in the browser.
                editor.shown = now
                node = arriving
                arriving = nil
                playhead = Playhead()
            }
            .onChange(of: undo, initial: true) { _, now in
                // The assistant's edits are each one step of this window's undo.
                if assistant == nil { assistant = Assistant(editor: editor) }
                assistant?.edited = { [weak document = self.document, weak now] before, files in
                    document?.took(before, files: files, undo: now)
                }
            }
            .task(id: editor.revision) {
                // Every state is linted once edits stop, as in the browser.
                try? await Task.sleep(for: .milliseconds(700))
                guard !Task.isCancelled, !editor.whole else { return }
                editor.lintEvery()
            }
    }

    /// `view` with what the window shows over it: an export in Quick Look or to save, the
    /// on-device model's answer, and why an edit was not made.
    private func presented(_ view: some View) -> some View {
        view
            .quickLookPreview($looking)
            .fileExporter(
                isPresented: exporting, document: saving, contentType: savingType, defaultFilename: savingName
            ) { result in
                if case .failure(let error) = result { failure = "\(error)" }
            }
            .sheet(item: $asked) { asked in
                OfferSheet(title: asked.title, ask: asked.ask, take: asked.take) { ops in
                    perform { try document.make(ops, undo: undo) }
                }
            }
            .alert("Not made", isPresented: failing, presenting: failure) { _ in
                Button("OK") { failure = nil }
            } message: { said in
                Text(said)
            }
    }

    /// Whether an export waits to be saved.
    private var exporting: Binding<Bool> {
        Binding(get: { saving != nil }, set: { if !$0 { saving = nil } })
    }

    /// Whether an alert says why an edit was not made.
    private var failing: Binding<Bool> {
        Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })
    }

    /// The canvas: the state shown on Metal, with what is selected on it.
    @ViewBuilder private var stage: some View {
        if let shown {
            let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
            // Drawn again after each edit, and as a drag moves a node or shows its patch (PLAN 3.7).
            ScaenaCanvas(
                session: editor.session, state: shown, revision: editor.revision &+ editor.drawn, playhead: $playhead
            )
            .overlay {
                CanvasSelection(editor: editor, state: shown, size: size, node: $node) { ops in
                    perform { try document.make(ops, undo: undo) }
                }
            }
            .aspectRatio(size.width / max(size.height, 1), contentMode: .fit)
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ContentUnavailableView("No states", systemImage: "rectangle.stack")
        }
    }

    /// Show what `finding` is about: its state, and its node selected.
    private func go(_ finding: Finding) {
        if let state = finding.state, state != shown {
            arriving = finding.node
            chosen = state
        } else {
            node = finding.node
        }
    }

    /// `state` at rest as a PNG the canvas's width, a pixel to the unit.
    private func png(_ state: String) -> Exporting.Kind {
        let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
        return .png(state: state, width: Int(size.width.rounded()))
    }

    /// Write `kind` and share it, or show it in Quick Look.
    private func export(_ kind: Exporting.Kind, sharing: Bool) {
        perform {
            let url = try Exporting.file(kind, of: editor)
            if sharing { Exporting.share(url) } else { looking = url }
        }
    }

    /// Offer `kind` to save where the user says.
    private func save(_ kind: Exporting.Kind) {
        perform {
            let data = try Exporting.bytes(kind, of: editor)
            savingType = kind.type
            savingName = Exporting.name(kind, of: editor)
            saving = ExportedFile(data: data)
        }
    }

    /// What the window shows, which a question to the assistant begins with: the state shown,
    /// and the node selected.
    private func seeing() -> Seeing? {
        guard let shown else { return nil }
        var nodes: [Seeing.Selected] = []
        if let node {
            nodes.append(Seeing.Selected(node: node, type: (try? editor.session.choices(state: shown, node: node))?.type))
        }
        return Seeing(state: shown, nodes: nodes)
    }

    /// `finding` explained by the on-device model.
    private func explain(_ finding: Finding) {
        var words: String?
        if let state = finding.state, let node = finding.node {
            words = (try? editor.session.text(state: state, node: node)) ?? nil
        }
        let text = words
        asked = Asked(title: "\(finding.code), explained", ask: { try await OnDevice.explain(finding, text: text) }, take: nil)
    }

    private func make(_ op: JSONValue) {
        perform { try document.make([op], undo: undo) }
    }

    private func perform(_ edit: () throws -> Void) {
        do {
            try edit()
        } catch {
            failure = "\(error)"
        }
    }
}
