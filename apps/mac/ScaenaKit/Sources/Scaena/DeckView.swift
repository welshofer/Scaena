import QuickLook
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

/// A deck's window (PLAN 3.3–3.4): its states down the side, each drawn small; the state chosen
/// painted by the engine on Metal, a node selected on it by a click, and its cue under it, to
/// play and scrub; what lint found, each with its fix; the deck's `.scn` beside the canvas; and
/// the inspector, which edits the node selected, or the state with none; and the assistant, which
/// edits it with the user (PLAN 3.6). A double click types in a text on the canvas (PLAN 3.9).
/// The Node menu inserts what the theme and the bundle offer, copies, deletes, and locks a node
/// (PLAN 3.11); Copy, Cut, and Paste take a node as a clip, and paste a picture, a sheet's cells,
/// or words another app copied, and ⌥⌘C and ⌥⌘V a look (PLAN 3.12). Every edit is a patch or a
/// source, as in the browser, and one step to undo; a burst of typing is one.
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
    /// Text typed in place on the canvas.
    @State private var typing: Typing?
    /// Where the pointer last pressed on the canvas, canvas units: where Insert puts what it adds.
    @State private var pointed: CGPoint?
    /// What the canvas says of an edit its keys or the Node menu made.
    @State private var said: String?
    /// What the deck may have inserted, read again after each edit.
    @State private var inserts: [Insert] = []
    /// Whether the node selected is locked by its own lock: what Lock undoes.
    @State private var locked = false
    /// The look ⌥⌘C copied last, which ⌥⌘V pastes, as `look` gives it.
    @State private var copiedLook: JSONValue?
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
            .focusedSceneValue(\.deck, actions)
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
            Inspector(editor: editor, state: shown, node: $node, typing: typing, offer: { asked = $0 }) { ops in
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
                typing?.sync(shown: now)
            }
            .onChange(of: undo, initial: true) { _, now in
                // The assistant's edits are each one step of this window's undo.
                if assistant == nil { assistant = Assistant(editor: editor) }
                assistant?.edited = { [weak document = self.document, weak now] before, files in
                    document?.took(before, files: files, undo: now)
                }
                // Text typed on the canvas: a burst of typing is one step of it (PLAN 3.9).
                if typing == nil { typing = Typing(editor: editor) }
                typing?.edited = { [weak document = self.document, weak now] before, joins in
                    document?.typedInPlace(before, joins: joins, undo: now)
                }
            }
            .task(id: "\(shown ?? "")\u{1f}\(node ?? "")\u{1f}\(editor.revision)") {
                // What the Node menu offers, read again as the deck or the selection changes.
                inserts = (try? editor.session.inserts()) ?? []
                let boxes = shown.flatMap { try? editor.session.boxes(state: $0) } ?? []
                locked = boxes.contains { $0.node == node && $0.locked == node }
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
                if let typing {
                    CanvasSelection(
                        editor: editor, state: shown, size: size, node: $node, typing: typing, pointed: $pointed,
                        said: $said, delete: delete, clip: clip
                    ) { ops in
                        perform { try document.make(ops, undo: undo) }
                    }
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

    /// What the Node menu does in this window (PLAN 3.11): nothing while a text is typed in, whose
    /// keys are the text's.
    private var actions: DeckActions? {
        guard let shown, typing?.typing != true else { return nil }
        let selected = node
        return DeckActions(
            inserts: inserts,
            insert: { n in insert(n, in: shown) },
            duplicate: selected.map { node in { duplicate(node, in: shown) } },
            delete: selected == nil ? nil : delete,
            lock: selected.map { node in { lock(node) } },
            locked: locked,
            copyLook: selected.map { node in { copyLook(node, in: shown) } },
            pasteLook: copiedLook != nil && selected != nil ? { pasteLook(in: shown) } : nil)
    }

    /// The Edit menu's Copy, Cut, and Paste on the canvas (PLAN 3.12).
    private func clip(_ what: CanvasKeys.Clipping) {
        switch what {
        case .copy: copy(cut: false)
        case .cut: copy(cut: true)
        case .paste: paste()
        }
    }

    /// ⌘C, and ⌘X with `cut`: the node selected onto the pasteboard as a clip, and as its text,
    /// which pastes in this deck or another, or the browser's (PLAN 2.37); a cut then takes it out
    /// of the state shown on, as Delete does.
    private func copy(cut: Bool) {
        guard let shown, let node else {
            said = "nothing selected to \(cut ? "cut" : "copy")"
            return
        }
        do {
            Pasteboard.write(clip: try editor.session.copying(state: shown, nodes: [node]))
        } catch {
            said = "not copied: \(error)"
            return
        }
        said = "\(node) \(cut ? "cut" : "copied"): ⌘V pastes it, in this deck or another"
        if cut { delete(false) }
    }

    /// ⌘V: what the pasteboard holds, where the pointer last pressed on the canvas, as Insert
    /// places a node (PLAN 2.37, 2.96): a clip's nodes under ids new to the deck; a picture as an
    /// image; a data file as a source and a chart of it; a sheet's cells as a source and the table
    /// they were; other words as a text in the theme's body role. Each enters in the state shown,
    /// selected: a step to undo for the source a file declares, and one for what is pasted.
    private func paste() {
        guard let shown else { return }
        let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
        let at = pointed ?? CGPoint(x: size.width / 2, y: size.height / 2)
        switch Pasteboard.read() {
        case .clip(let text):
            pasteText(text, in: shown, at: at)
        case .words(let text):
            if let cells = (try? editor.session.cells(text)) ?? nil {
                attach(Data(cells.csv.utf8), named: "\(cells.name).csv", cells: cells, in: shown, at: at)
            } else {
                pasteText(text, in: shown, at: at)
            }
        case .file(let data, let name):
            let ext = (name as NSString).pathExtension.lowercased()
            if ext == "csv" || ext == "json" {
                attach(data, named: name, cells: nil, in: shown, at: at)
            } else if ["png", "jpg", "jpeg"].contains(ext) {
                picture(data, named: name, in: shown, at: at)
            } else if let png = Pasteboard.png(data) {
                picture(png, named: "\((name as NSString).deletingPathExtension).png", in: shown, at: at)
            } else {
                said = "\(name) is neither a picture (PNG, JPEG) nor data (CSV, JSON): nothing was added"
            }
        case nil:
            said = "nothing on the clipboard to paste"
        }
    }

    /// A clip's text, or words, pasted in `state` about `at`.
    private func pasteText(_ text: String, in state: String, at: CGPoint) {
        perform {
            let pasted = try editor.session.pasting(text, state: state, at: at)
            try document.make(pasted.patch, undo: undo)
            node = pasted.id
            said = (["\(pasted.ids.joined(separator: ", ")) pasted in \(state)"] + pasted.lacked).joined(separator: "; ")
        }
    }

    /// A picture kept in the bundle, by its content, and inserted as an image about `at`.
    private func picture(_ data: Data, named name: String, in state: String, at: CGPoint) {
        perform {
            let path = try editor.session.drop(data, named: name)
            let offered = try editor.session.inserts()
            guard let n = offered.firstIndex(where: { $0.node["type"]?.string == "image" && $0.node["src"]?.string == path })
            else {
                said = "\(path) is no image the deck can insert: a PNG or a JPEG"
                return
            }
            let added = try editor.session.inserting(state: state, n: n, at: at, named: name)
            try document.make(added.patch, undo: undo)
            node = added.id
            said = "\(name) pasted as \(added.id), kept as \(path)"
        }
    }

    /// A data file kept in the bundle and declared as a source (PLAN 2.76), then a chart of it
    /// inserted about `at`; a sheet's `cells`, typed as they read, and the table they were, each
    /// column printing its figures as they were copied (PLAN 2.96).
    private func attach(_ data: Data, named name: String, cells: Cells?, in state: String, at: CGPoint) {
        perform {
            let path = try editor.session.drop(data, named: name)
            let attaching = try editor.session.attaching(path: path, schema: cells?.schema)
            if !attaching.patch.isEmpty { try document.make(attaching.patch, undo: undo) }
            let kind = cells == nil ? "chart" : "table"
            let offered = try editor.session.inserts()
            guard
                let n = offered.firstIndex(where: {
                    $0.node["type"]?.string == kind && $0.node["data"]?.string == "@\(attaching.data)"
                })
            else {
                said = "@\(attaching.data) attached: Insert offers no \(kind) of it"
                return
            }
            let with: JSONValue? = cells.map { ["columns": $0.tableColumns] }
            let added = try editor.session.inserting(state: state, n: n, at: at, with: with)
            try document.make(added.patch, undo: undo)
            node = added.id
            said = "a \(kind) of @\(attaching.data) pasted as \(added.id), its data kept as \(path)"
        }
    }

    /// ⌥⌘C: `node`'s look as `state` shows it, copied for ⌥⌘V (PLAN 2.58).
    private func copyLook(_ node: String, in state: String) {
        perform {
            let look = try editor.session.look(state: state, node: node)
            guard look["props"]?.array?.isEmpty == false else {
                said = "\(node) has no look of its own to copy"
                return
            }
            copiedLook = look
            said = "\(node)'s look copied: ⌥⌘V pastes it on what is selected"
        }
    }

    /// ⌥⌘V: the look copied pasted on the node selected, in the state shown: one patch of
    /// `choose`s, each written where that node's own value lives (PLAN 2.58).
    private func pasteLook(in state: String) {
        guard let look = copiedLook, let node else { return }
        let from = look["node"]?.string ?? "the"
        perform {
            let put = try editor.session.putting(state: state, look: look, nodes: [node])
            let others = (put.same.isEmpty ? [] : ["\(node) looks so already"]) + put.refused.map { "\($0.node): \($0.why)" }
            guard !put.patch.isEmpty else {
                said = others.isEmpty ? "nothing takes \(from)'s look" : others.joined(separator: "; ")
                return
            }
            try document.make(put.patch, undo: undo)
            said = (["\(from)'s look pasted on \(put.took.joined(separator: ", "))"] + others).joined(separator: "; ")
        }
    }

    /// Insert what the deck offers `n`th where the pointer last pressed on the canvas, or in its
    /// middle; in the room nearest there where content would overlap (PLAN 2.34, 2.79). It enters
    /// in the state shown, selected: one step to undo.
    private func insert(_ n: Int, in state: String) {
        let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
        let at = pointed ?? CGPoint(x: size.width / 2, y: size.height / 2)
        perform {
            let added = try editor.session.inserting(state: state, n: n, at: at)
            try document.make(added.patch, undo: undo)
            node = added.id
            said = "\(inserts.indices.contains(n) ? inserts[n].label : "it") inserted as \(added.id), in \(state)"
        }
    }

    /// A copy of `node` beside it, selected (⌘D): one step to undo.
    private func duplicate(_ node: String, in state: String) {
        perform {
            let added = try editor.session.duplicating(state: state, node: node)
            try document.make(added.patch, undo: undo)
            self.node = added.id
            said = "\(node) copied as \(added.id)"
        }
    }

    /// Take the node selected out of the state shown and the states after it, and out of the deck
    /// where no state shows it after; or, `everywhere`, out of the deck (Delete, Shift+Delete). A
    /// locked node is not taken: one step to undo.
    private func delete(_ everywhere: Bool) {
        guard let shown, let node else {
            said = "nothing selected to delete"
            return
        }
        let boxes = (try? editor.session.boxes(state: shown)) ?? []
        if let holder = boxes.first(where: { $0.node == node })?.locked {
            let by = holder == node ? "" : " by \(holder)"
            said = "\(node) is locked\(by): nothing deleted · ⇧⌘L unlocks it"
            return
        }
        perform {
            let ops = try editor.session.deleting(state: shown, node: node, everywhere: everywhere)
            let gone = ops.allSatisfy { $0["op"]?.string == "remove_node" }
            try document.make(ops, undo: undo)
            self.node = nil
            said = gone ? "\(node) deleted from the deck" : "\(node) deleted from \(shown) on"
        }
    }

    /// Lock `node` by its own lock, or unlock it (⇧⌘L, PLAN 2.95): the canvas passes over a node
    /// locked, in every state. One step to undo.
    private func lock(_ node: String) {
        let (ops, locks) = ScaenaKit.locking([node], own: { _ in locked })
        guard !ops.isEmpty else { return }
        perform {
            try document.make(ops, undo: undo)
            said = locks ? "\(node) locked: the canvas passes over it · ⇧⌘L unlocks it" : "\(node) unlocked"
        }
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
