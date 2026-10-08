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
/// or words another app copied, and ⌥⌘C and ⌥⌘V a look (PLAN 3.12). The state list adds, renames,
/// moves, and removes states; Slides shows the light table in place of the canvas, and Rehearse
/// plays the deck there, keeping the time each state took; the cue's bars are dragged to time it
/// (PLAN 3.14). Every edit is a patch or a source, as in the browser, and one step to undo; a burst
/// of typing is one.
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
    /// The others selected with the node selected, children of the same container (PLAN 3.13).
    @State private var also: [String] = []
    /// The nodes the state shown has locked by their own lock: what Lock undoes.
    @State private var ownLocks: Set<String> = []
    /// Whether the node selected is a group, which Ungroup takes apart.
    @State private var grouped = false
    /// The look ⌥⌘C copied last, which ⌥⌘V pastes, as `look` gives it.
    @State private var copiedLook: JSONValue?
    /// What the on-device model is asked, shown in a sheet.
    @State private var asked: Asked?
    /// The light table shown in place of the canvas, and the slides selected on it (PLAN 3.14).
    @State private var showsSlides = false
    @State private var slidesPicked: Set<String> = []
    /// A rehearsal running in place of the canvas, and one over, what it kept shown in a sheet.
    @State private var rehearsal: Rehearsal?
    @State private var rehearsed: Rehearsal?
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
    /// The panel the inspector's column shows (PLAN 3.15).
    @SceneStorage("panel") private var panel: SidePanel = .inspector

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
            StateList(editor: editor, chosen: $chosen, make: restage, add: addState)
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
                middle
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
            SidePanels(editor: editor, panel: $panel, edits: panelEdits) {
                Inspector(
                    editor: editor, state: shown, node: $node, also: $also, typing: typing, offer: { asked = $0 }
                ) { ops in
                    perform { try document.make(ops, undo: undo) }
                }
            }
            .inspectorColumnWidth(min: 240, ideal: 320, max: 520)
        }
    }

    /// The canvas and its cue; or the light table, or a rehearsal, in its place (PLAN 3.14).
    @ViewBuilder private var middle: some View {
        if let running = Binding($rehearsal) {
            RehearsalStage(editor: editor, rehearsal: running) {
                rehearsed = rehearsal
                rehearsal = nil
            }
        } else if showsSlides {
            LightTable(editor: editor, picked: $slidesPicked, show: showState) { ops, then in
                perform {
                    try document.make(ops, undo: undo)
                    slidesPicked = Set(then)
                }
            }
        } else {
            VStack(spacing: 0) {
                stage
                if let shown {
                    Divider()
                    CueBar(
                        editor: editor, state: shown, node: node, playhead: $playhead, make: timing
                    ) { picked in
                        node = picked
                        also = []
                    }
                }
            }
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
            Button {
                showsSlides = false
                rehearsal = Rehearsal(slots: editor.slots)
            } label: {
                Label("Rehearse", systemImage: "stopwatch")
            }
            .keyboardShortcut("r", modifiers: [.option, .command])
            .help("Play the deck as presented, here, and keep the time each state takes as its hold (⌥⌘R)")
            .disabled(editor.slots.isEmpty || rehearsal != nil || !editor.valid)
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
            Toggle(isOn: $showsSlides) {
                Label("Slides", systemImage: "square.grid.3x2")
            }
            .keyboardShortcut("l", modifiers: [.option, .command])
            .help("Every slide in place of the canvas, to reorder, copy, and take out (⌥⌘L)")
            .disabled(rehearsal != nil)
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
            .help("The node selected, or the state; the theme, the data, the files, and the versions")
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
                also = []
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
                ownLocks = Set(boxes.filter { $0.locked == $0.node }.map(\.node))
                if let shown, let node, let choices = try? editor.session.choices(state: shown, node: node) {
                    grouped = choices.type == "group"
                } else {
                    grouped = false
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
            .sheet(isPresented: showingRehearsed) {
                if let done = rehearsed {
                    RehearsedSheet(rehearsal: done) { keep(done) }
                }
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

    /// Whether what a rehearsal kept is shown.
    private var showingRehearsed: Binding<Bool> {
        Binding(get: { rehearsed != nil }, set: { if !$0 { rehearsed = nil } })
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
                        editor: editor, state: shown, size: size, node: $node, also: $also, typing: typing, pointed: $pointed,
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
            also = []
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

    /// Every node selected, the first first (PLAN 3.13).
    private var selection: [String] { node.map { [$0] + also } ?? [] }

    /// What the Node menu does in this window (PLAN 3.11, 3.13): nothing while a text is typed in,
    /// whose keys are the text's.
    private var actions: DeckActions? {
        if showsSlides { return slideActions }
        guard let shown, typing?.typing != true, rehearsal == nil else { return nil }
        let some = !selection.isEmpty
        let one = node != nil && also.isEmpty
        return DeckActions(
            inserts: inserts,
            insert: { n in insert(n, in: shown) },
            duplicate: some ? { duplicate(in: shown) } : nil,
            delete: some ? delete : nil,
            lock: some ? { lock() } : nil,
            locked: some && selection.allSatisfy(ownLocks.contains),
            copyLook: one ? { copyLook(node ?? "", in: shown) } : nil,
            pasteLook: copiedLook != nil && some ? { pasteLook(in: shown) } : nil,
            group: selection.count > 1 ? { group(in: shown) } : nil,
            ungroup: one && grouped ? { ungroup(in: shown) } : nil,
            order: some ? { how in order(how, in: shown) } : nil)
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
        let nodes = selection
        guard let shown, !nodes.isEmpty else {
            said = "nothing selected to \(cut ? "cut" : "copy")"
            return
        }
        do {
            Pasteboard.write(clip: try editor.session.copying(state: shown, nodes: nodes))
        } catch {
            said = "not copied: \(error)"
            return
        }
        let them = nodes.count > 1 ? "\(nodes.count) selected" : nodes[0]
        said = "\(them) \(cut ? "cut" : "copied"): ⌘V pastes \(nodes.count > 1 ? "them" : "it"), in this deck or another"
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
            also = pasted.also
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
            also = []
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
            also = []
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

    /// ⌥⌘V: the look copied pasted on each node selected, in the state shown: one patch of
    /// `choose`s, each written where that node's own value lives (PLAN 2.58).
    private func pasteLook(in state: String) {
        let nodes = selection
        guard let look = copiedLook, !nodes.isEmpty else { return }
        let from = look["node"]?.string ?? "the"
        perform {
            let put = try editor.session.putting(state: state, look: look, nodes: nodes)
            let same = put.same.isEmpty ? [] : ["\(put.same.joined(separator: ", ")) look so already"]
            let others = same + put.refused.map { "\($0.node): \($0.why)" }
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
            also = []
            said = "\(inserts.indices.contains(n) ? inserts[n].label : "it") inserted as \(added.id), in \(state)"
        }
    }

    /// A copy of each node selected beside it, the copies selected (⌘D, PLAN 2.42): one patch, one
    /// step to undo.
    private func duplicate(in state: String) {
        let nodes = selection
        perform {
            let added = try nodes.map { try editor.session.duplicating(state: state, node: $0) }
            let ids = added.map(\.id)
            guard Set(ids).count == ids.count else {
                said = "not copied: two copies would take one id, \(ids.joined(separator: ", "))"
                return
            }
            try document.make(added.flatMap(\.patch), undo: undo)
            node = ids.first
            also = Array(ids.dropFirst())
            said = "\(nodes.joined(separator: ", ")) copied as \(ids.joined(separator: ", "))"
        }
    }

    /// Take the nodes selected out of the state shown and the states after it, and out of the
    /// deck where no state shows them after; or, `everywhere`, out of the deck (Delete,
    /// Shift+Delete). Locked nodes are not taken: one patch, one step to undo.
    private func delete(_ everywhere: Bool) {
        let nodes = selection
        guard let shown, !nodes.isEmpty else {
            said = "nothing selected to delete"
            return
        }
        let boxes = (try? editor.session.boxes(state: shown)) ?? []
        if let held = boxes.first(where: { nodes.contains($0.node) && $0.locked != nil }), let holder = held.locked {
            let by = holder == held.node ? "" : " by \(holder)"
            said = "\(held.node) is locked\(by): nothing deleted · ⇧⌘L unlocks it"
            return
        }
        perform {
            let ops = try nodes.flatMap { try editor.session.deleting(state: shown, node: $0, everywhere: everywhere) }
            let gone = ops.allSatisfy { $0["op"]?.string == "remove_node" }
            try document.make(ops, undo: undo)
            node = nil
            also = []
            let them = nodes.joined(separator: ", ")
            said = gone ? "\(them) deleted from the deck" : "\(them) deleted from \(shown) on"
        }
    }

    /// Lock the nodes selected by their own lock, or unlock them where each is so locked
    /// (⇧⌘L, PLAN 2.95): the canvas passes over a node locked, in every state. One step to undo.
    private func lock() {
        let nodes = selection
        let (ops, locks) = ScaenaKit.locking(nodes, own: { ownLocks.contains($0) })
        guard !ops.isEmpty else { return }
        perform {
            try document.make(ops, undo: undo)
            let them = nodes.joined(separator: ", ")
            said = locks ? "\(them) locked: the canvas passes over it · ⇧⌘L unlocks" : "\(them) unlocked"
        }
    }

    /// ⌘G: the nodes selected in a new group where they stand, the group selected (PLAN 2.43).
    private func group(in state: String) {
        let nodes = selection
        perform {
            let grouping = try editor.session.grouping(state: state, nodes: nodes)
            try document.make(grouping.patch, undo: undo)
            node = grouping.id
            also = []
            said = "\(nodes.joined(separator: ", ")) grouped as \(grouping.id)"
        }
    }

    /// ⌘⇧G: the group selected taken apart, what it held selected (PLAN 2.43).
    private func ungroup(in state: String) {
        guard let group = node else { return }
        let held = ((try? editor.session.boxes(state: state)) ?? []).filter { $0.parent == group }.map(\.node)
        perform {
            try document.make([["op": "ungroup", "group": .string(group)]], undo: undo)
            node = held.first
            also = Array(held.dropFirst())
            said = "\(group) taken apart: \(held.joined(separator: ", "))"
        }
    }

    /// ⌘] and ⌘[, with Option to the front and the back: the nodes selected ordered `how` among
    /// what their container paints (PLAN 2.42), one patch of `choose`s of `z`.
    private func order(_ how: String, in state: String) {
        let nodes = selection
        perform {
            guard let arranged = try editor.session.arranging(state: state, nodes: nodes, how: .order(how)),
                !arranged.patch.isEmpty
            else {
                said = "\(nodes.joined(separator: ", ")): nothing to bring \(how)"
                return
            }
            try document.make(arranged.patch, undo: undo)
            said = "\(nodes.joined(separator: ", ")) brought \(how)"
        }
    }

    /// What the Node menu does while the light table shows: Duplicate copies the slides selected,
    /// each just after itself, the copies selected (PLAN 2.97).
    private var slideActions: DeckActions {
        let picked = Slide.of(editor.slots).map(\.id).filter(slidesPicked.contains)
        let duplicate = {
            perform {
                try document.make(Restaging.duplicateSlides(picked), undo: undo)
                // Each copy is the slide just after the one it copies.
                let ids = Slide.of(editor.slots).map(\.id)
                slidesPicked = Set(
                    picked.compactMap { id -> String? in
                        guard let i = ids.firstIndex(of: id), i + 1 < ids.count else { return nil }
                        return ids[i + 1]
                    })
            }
        }
        return DeckActions(
            inserts: [], insert: { _ in }, duplicate: picked.isEmpty ? nil : duplicate, delete: nil, lock: nil,
            locked: false, copyLook: nil, pasteLook: nil, group: nil, ungroup: nil, order: nil)
    }

    /// What the panels do to the deck (PLAN 3.15): each edit one step of the window's undo.
    private var panelEdits: PanelEdits {
        PanelEdits(
            beside: { edit in perform { try document.beside(edit, undo: undo) } },
            data: { edit in perform { try document.data(edit, undo: undo) } })
    }

    /// The state list's patches (PLAN 3.14): one step to undo, then `then` shown.
    private func restage(_ ops: [JSONValue], then: String?) {
        perform {
            try document.make(ops, undo: undo)
            if let then { chosen = then }
        }
    }

    /// A step or a slide added after `state`, then shown (PLAN 2.35): one step to undo.
    private func addState(after state: String, as what: StateAdding) {
        perform {
            let added = try editor.session.addingState(after: state, as: what)
            try document.make(added.patch, undo: undo)
            chosen = added.id
            said = "\(added.id) added after \(what == .step ? state : "\(state)'s slide")"
        }
    }

    /// `state` shown on the canvas, the light table put away.
    private func showState(_ state: String) {
        showsSlides = false
        chosen = state
    }

    /// The cue's patches (PLAN 2.44): a bar timed, or a motion added, one step to undo; with no
    /// ops, only what the status says.
    private func timing(_ ops: [JSONValue], said words: String) {
        guard !ops.isEmpty else {
            said = words
            return
        }
        perform {
            try document.make(ops, undo: undo)
            said = words
        }
    }

    /// What a rehearsal kept, made the deck's: each state reached holding as long as it took, less
    /// its cue, one patch (PLAN 2.63).
    private func keep(_ done: Rehearsal) {
        let holds = done.holds
        guard !holds.isEmpty else { return }
        perform {
            try document.make(holds, undo: undo)
            said = "\(holds.count) hold\(holds.count == 1 ? "" : "s") from the rehearsal"
        }
    }

    private func perform(_ edit: () throws -> Void) {
        do {
            try edit()
        } catch {
            failure = "\(error)"
        }
    }
}
