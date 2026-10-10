import QuickLook
import ScaenaKit
import SwiftUI
import UniformTypeIdentifiers

/// A deck's window (PLAN 3.3–3.4), as a presentation app's (PLAN 3.18): the slides down the side,
/// numbered, each drawn small, a slide's steps under it; the slide chosen painted by the engine on
/// Metal, on the gray a slide sits on, an object selected on it by a click; the toolbar's View and
/// Zoom, Add Slide and Play, what may be inserted by kind, Share and the assistant, and Format,
/// Animate, and Document, the inspector's tabs; and the inspector, which edits the object
/// selected, or the slide with none (PLAN 3.6 for the assistant). What a person working on slides
/// does not need at first stays in the View menu, off: the cue as a timeline, the deck in its other
/// sizes, the grid, what lint found, and the deck's `.scn`. A double click types in a text on the
/// canvas (PLAN 3.9).
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
    /// Whether a node selected shows on another slide than the one shown (`showsElsewhere`).
    @State private var elsewhere = false
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
    /// Remote…: the code a remote joins with, shown in a sheet (PLAN 4.10).
    @State private var remoting = false
    /// An export shown in Quick Look (PLAN 3.8).
    @State private var looking: URL?
    /// An export to save where the user says, its type, and the name offered.
    @State private var saving: ExportedFile?
    @State private var savingType = UTType.pdf
    @State private var savingName = ""
    @SceneStorage("assistant") private var showsAssistant = false
    @SceneStorage("source") private var showsSource = false
    /// What lint found, under the canvas: off until asked for (PLAN 3.18).
    @SceneStorage("issues") private var showsFindings = false
    @SceneStorage("inspector") private var showsInspector = true
    /// The inspector's tab, and the Document tab's panel (PLAN 3.15, 3.18).
    @SceneStorage("tab") private var tab: InspectorTab = .format
    @SceneStorage("documentPanel") private var documentPanel: DocumentPanel = .theme
    /// The cue of the slide shown as a timeline under the canvas (PLAN 3.14), off until asked for.
    @SceneStorage("timeline") private var showsTimeline = false
    /// Whether the theme's grid is drawn over the canvas (PLAN 3.16).
    @SceneStorage("grid") private var showsGrid = false
    /// Whether the safe area's strip is drawn around the slide's edge (PLAN 3.29).
    @SceneStorage("safeArea") private var showsSafeArea = false
    /// Whether Add Slide's gallery of layouts is open (PLAN 3.30).
    @State private var choosingSlide = false
    /// Whether Choose Picture… asks for a file.
    @State private var choosingPicture = false
    /// The themes that ship, offered as a deck made by New opens (PLAN 3.3); none once one is chosen.
    @State private var offeredThemes: [ShippedTheme] = []
    /// The slides made in this window, from the gallery or after the slide shown (PLAN 3.30):
    /// only theirs show their layout's empty slots, so a deck opened shows none it never filled.
    @State private var fresh: Set<String> = []
    /// Whether the state shown is drawn in each of the deck's formats under the canvas (PLAN 3.16).
    @SceneStorage("formats") private var showsFormats = false
    /// How close the canvas is shown, and the part of it shown (PLAN 3.16).
    @State private var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080))
    /// Whether the find bar shows over the canvas (PLAN 3.16).
    @State private var searching = false

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
            .focusedSceneValue(\.canvasActions, canvasActions)
            .focusedSceneValue(\.panes, panes)
    }

    /// The states down the side; beside them the canvas and the rest; and the toolbar.
    private var window: some View {
        NavigationSplitView {
            StateList(editor: editor, chosen: $chosen, make: restage, add: addState)
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
        } detail: {
            detail
        }
        #if os(macOS)
        .toolbar(id: "deck") { customizable }
        #else
        .toolbar { toolbar }
        #endif
        // A picture, or a sheet's data, chosen from a file lands as it lands dropped: where the
        // pointer last pressed, or in the middle of the slide.
        .fileImporter(
            isPresented: $choosingPicture, allowedContentTypes: [.image, .commaSeparatedText, .json]
        ) { result in
            switch result {
            case .success(let url): choose(url)
            case .failure(let error): failure = error.localizedDescription
            }
        }
    }

    /// The file at `url`, chosen, landed on the slide shown as a drop lands it.
    private func choose(_ url: URL) {
        guard let shown else { return }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        guard let data = try? Data(contentsOf: url) else {
            failure = "\(url.lastPathComponent) could not be read"
            return
        }
        let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
        let at = pointed ?? CGPoint(x: size.width / 2, y: size.height / 2)
        land(data, named: url.lastPathComponent, in: shown, at: at)
    }

    /// The canvas and the panes beside it, and the inspector. On the Mac the inspector is a column
    /// of the window's own, as a presentation app's is (PLAN 3.27): the system's inspector is a
    /// split view under the slides' floating column, and each split view there counts that
    /// column's width again in the narrowest the window may be.
    @ViewBuilder private var detail: some View {
        #if os(macOS)
        HStack(spacing: 0) {
            divided
            if showsInspector {
                Divider()
                inspectorColumn
                    .frame(width: 300)
                    .background(Color(nsColor: .windowBackgroundColor))
            }
        }
        #else
        divided
            .inspector(isPresented: $showsInspector) {
                inspectorColumn
                    .inspectorColumnWidth(min: 260, ideal: 300, max: 480)
            }
        #endif
    }

    /// The inspector's tabs: the object selected, or the slide with none, and the deck as a whole.
    private var inspectorColumn: some View {
        InspectorColumn(editor: editor, tab: $tab, document: $documentPanel, edits: panelEdits) {
            inspecting(.format)
        } animate: {
            inspecting(.animate)
        }
    }

    /// The source, the canvas with its cue and the findings, and the assistant.
    private var divided: some View {
        Panes(axis: .horizontal) {
            if showsSource {
                SourcePane(text: editor.source) { typed in document.type(typed, undo: undo) }
                    .frame(minWidth: 280, idealWidth: 420)
            }
            Panes(axis: .vertical) {
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
    }

    /// The inspector under `tab`: the object selected, or the slide with none.
    private func inspecting(_ tab: InspectorTab) -> some View {
        Inspector(
            editor: editor, state: shown, node: $node, also: $also, typing: typing, tab: tab, offer: { asked = $0 }
        ) { ops in
            perform { try document.make(ops, undo: undo) }
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
                if searching {
                    FindBar(editor: editor, reveal: reveal, make: { ops in perform { try document.make(ops, undo: undo) } }) {
                        searching = false
                    }
                    Divider()
                }
                stage
                if showsFormats, let shown {
                    Divider()
                    FormatsStrip(editor: editor, state: shown) { format in showFormat(format) }
                }
                if showsTimeline, let shown {
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

    /// The toolbar, as a presentation app's (PLAN 3.18): what the window shows and how close, and on
    /// the iPad Undo (PLAN 4.11); a slide added, and the deck played; a text, a shape, an image, a
    /// chart, or a table inserted; the deck shared, and the assistant; and the inspector's tabs.
    /// Every item is in the menus too, with its key there.
    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .navigation) {
            viewing
            zooming
            #if !os(macOS)
            // Undo by touch, as a presentation app's on the iPad (PLAN 4.11).
            UndoButton(undo: undo)
            #endif
        }
        ToolbarItemGroup {
            adding
            playing
        }
        ToolbarItemGroup {
            ForEach(Insertable.allCases) { inserting($0) }
        }
        ToolbarItemGroup {
            sharing
            assisting
        }
        ToolbarItemGroup {
            ForEach(InspectorTab.allCases) { tabbing($0) }
        }
    }

    #if os(macOS)
    /// The toolbar on the Mac (PLAN 3.27): the same items, each its own, as a Mac window's toolbar
    /// has them, so the window narrows as a Mac window does. What does not fit goes into the
    /// toolbar's overflow menu, rather than holding the window as wide as every item, and View ›
    /// Customize Toolbar… takes away what a person does not want.
    @ToolbarContentBuilder private var customizable: some CustomizableToolbarContent {
        Group {
            ToolbarItem(id: "view", placement: .navigation) { viewing }
            ToolbarItem(id: "zoom", placement: .navigation) { zooming }
            ToolbarItem(id: "add") { adding }
            ToolbarItem(id: "play") { playing }
        }
        Group {
            ToolbarItem(id: "insert-text") { inserting(.text) }
            ToolbarItem(id: "insert-shape") { inserting(.shape) }
            ToolbarItem(id: "insert-image") { inserting(.image) }
            ToolbarItem(id: "insert-chart") { inserting(.chart) }
            ToolbarItem(id: "insert-table") { inserting(.table) }
        }
        Group {
            ToolbarItem(id: "share") { sharing }
            ToolbarItem(id: "assistant") { assisting }
            ToolbarItem(id: "format") { tabbing(.format) }
            ToolbarItem(id: "animate") { tabbing(.animate) }
            ToolbarItem(id: "document") { tabbing(.document) }
        }
    }
    #endif

    /// Play the slideshow from the slide shown.
    private var playing: some View {
        Button {
            Presenting.play(editor, from: shown)
        } label: {
            Label("Play", systemImage: "play.fill")
        }
        .help("Play the slideshow from this slide (⌥⌘P)")
        .disabled(editor.slots.isEmpty)
    }

    /// A kind of thing the toolbar inserts.
    private enum Insertable: CaseIterable, Identifiable {
        case text, shape, image, chart, table
        var id: Self { self }
    }

    /// The toolbar's menu that inserts `kind`.
    private func inserting(_ kind: Insertable) -> InsertMenu {
        switch kind {
        case .text:
            InsertMenu(title: "Text", symbol: "textbox", kinds: ["Text"], first: "body", inserts: inserts, insert: insertHere)
        case .shape:
            InsertMenu(
                title: "Shape", symbol: "square.on.circle", kinds: ["Shape", "Shader"], first: nil, inserts: inserts,
                insert: insertHere)
        case .image:
            InsertMenu(
                title: "Image", symbol: "photo", kinds: ["Image"], first: nil, inserts: inserts, insert: insertHere,
                choose: shown == nil ? nil : (title: "Choose Picture…", action: { choosingPicture = true }))
        case .chart:
            InsertMenu(title: "Chart", symbol: "chart.bar", kinds: ["Chart"], first: nil, inserts: inserts, insert: insertHere)
        case .table:
            InsertMenu(title: "Table", symbol: "tablecells", kinds: ["Table"], first: nil, inserts: inserts, insert: insertHere)
        }
    }

    /// The assistant, shown or put away.
    private var assisting: some View {
        Toggle(isOn: $showsAssistant) {
            Label("Assistant", systemImage: "sparkles")
        }
        .help("Ask about the deck, or for an edit, with your own key (⌥⌘A)")
    }

    /// The inspector shown at `tab`, or put away.
    private func tabbing(_ tab: InspectorTab) -> some View {
        Toggle(isOn: showing(tab)) {
            Label(tab.title, systemImage: tab.symbol)
        }
        .help(tab.help)
    }

    /// What the window shows: the slide, or every slide on the light table; and, off until asked
    /// for, the slide's timeline, its other sizes, the grid, the safe area, what lint found, and
    /// the source.
    private var viewing: some View {
        Menu {
            Toggle("Light Table", isOn: $showsSlides)
                .disabled(rehearsal != nil)
            Divider()
            Toggle("Timeline", isOn: $showsTimeline)
            Toggle("Other Sizes", isOn: $showsFormats)
            Toggle("Grid", isOn: $showsGrid)
            Toggle("Safe Area", isOn: $showsSafeArea)
            Divider()
            Toggle("Issues", isOn: $showsFindings)
            Toggle("Source", isOn: $showsSource)
        } label: {
            Label("View", systemImage: "rectangle.on.rectangle")
        }
        // Read by its name, not its symbol's: VoiceOver would say Screen Sharing.
        .accessibilityLabel("View")
        .help("The light table, and what the window shows beside the slide")
    }

    /// How close the slide is shown: the whole of it, or closer.
    private var zooming: some View {
        Menu {
            Button("Fit Slide") { zoom.fit() }
            Divider()
            ForEach([1.5, 2, 3, 4], id: \.self) { level in
                let percent = "\(Int(level * 100))%"
                Button(percent) { zoom.zoom(to: level) }
            }
            Divider()
            Button("Zoom In") { zoom.step(1) }
            Button("Zoom Out") { zoom.step(-1) }
                .disabled(zoom.level <= 1 + 1e-9)
        } label: {
            // How close, in words: a presentation app's zoom says its percentage.
            Text(zoomed)
                .monospacedDigit()
                .frame(minWidth: 40)
        }
        .accessibilityLabel("Zoom")
        .accessibilityValue(zoomed)
        .help("How close the slide is shown (⌘=, ⌘−, ⌘0)")
        .disabled(shown == nil || showsSlides)
    }

    /// How close the slide is shown, as the Zoom menu says it.
    private var zoomed: String {
        zoom.level > 1 + 1e-9 ? "\(Int((zoom.level * 100).rounded()))%" : "Fit"
    }

    /// A slide added after the one shown, then shown, from the gallery of the theme's layouts
    /// (PLAN 3.30); or a step of it.
    private var adding: some View {
        Button {
            choosingSlide = true
        } label: {
            Label("Add Slide", systemImage: "plus.rectangle.on.rectangle")
        }
        .help("Add a slide after this one, in one of the theme's layouts, or a step of this slide")
        .disabled(shown == nil)
        .accessibilityIdentifier("add-slide")
        .popover(isPresented: $choosingSlide, arrowEdge: .bottom) {
            if let shown {
                SlideGallery(editor: editor, state: shown) { layout in
                    choosingSlide = false
                    startSlide(after: shown, layout: layout)
                } step: {
                    choosingSlide = false
                    addState(after: shown, as: .step)
                }
            }
        }
    }

    /// The deck as a PDF, or the slide shown as a PNG: shared, looked at, or saved.
    private var sharing: some View {
        Menu {
            Button("Share as PDF…") { export(.pdf, sharing: true) }
            Button("Quick Look the PDF") { export(.pdf, sharing: false) }
            Button("Save as PDF…") { save(.pdf) }
            if let shown {
                Divider()
                Button("Share This Slide as PNG…") { export(png(shown), sharing: true) }
                Button("Save This Slide as PNG…") { save(png(shown)) }
            }
        } label: {
            Label("Share", systemImage: "square.and.arrow.up")
        }
        .accessibilityLabel("Share")
        .help("The deck as a PDF, or this slide as a picture")
        .disabled(editor.slots.isEmpty)
    }

    /// The inspector shown at `tab`, or put away where it shows that tab already.
    private func showing(_ tab: InspectorTab) -> Binding<Bool> {
        Binding(
            get: { showsInspector && self.tab == tab },
            set: { on in
                if on {
                    self.tab = tab
                    showsInspector = true
                } else {
                    showsInspector = false
                }
            })
    }

    /// What the View, Slide, and Play menus do in this window (PLAN 3.18).
    private var panes: WindowPanes {
        var made = WindowPanes(
            slides: $showsSlides, timeline: $showsTimeline, formats: $showsFormats, grid: $showsGrid,
            safeArea: $showsSafeArea, issues: $showsFindings, source: $showsSource, assistant: $showsAssistant,
            inspector: $showsInspector,
            tab: $tab,
            play: editor.slots.isEmpty ? nil : { Presenting.play(editor, from: shown) },
            rehearse: editor.slots.isEmpty || rehearsal != nil || !editor.valid ? nil : { rehearse() },
            remote: { remoting = true })
        // The Slide menu acts on the slide shown, as the slide list's menu acts on its slide.
        if let shown, rehearsal == nil, let slide = editor.slots.first(where: { $0.state == shown })?.slide {
            made.newSlide = { addState(after: shown, as: .slide) }
            made.newStep = { addState(after: shown, as: .step) }
            made.duplicateSlide = { duplicateSlide(slide) }
            if Slide.of(editor.slots).count > 1 { made.deleteSlide = { deleteSlide(slide) } }
        }
        return made
    }

    /// A copy of `slide` just after it, shown (PLAN 3.14): one step to undo.
    private func duplicateSlide(_ slide: String) {
        perform {
            try document.make(Restaging.duplicateSlides([slide]), undo: undo)
            undo?.setActionName("Duplicate Slide")
            // The copy is the slide just after the one it copies.
            let slides = Slide.of(editor.slots)
            if let i = slides.firstIndex(where: { $0.id == slide }), i + 1 < slides.count,
                let first = slides[i + 1].states.first
            {
                chosen = first
            }
            said = "Slide duplicated"
        }
    }

    /// `slide` taken out, with its steps, and the slide after it shown, or, past the last, the one
    /// before (PLAN 3.14): one step to undo.
    private func deleteSlide(_ slide: String) {
        let slides = Slide.of(editor.slots)
        guard slides.count > 1, let i = slides.firstIndex(where: { $0.id == slide }) else { return }
        let next = i + 1 < slides.count ? slides[i + 1] : slides[i - 1]
        perform {
            try document.make(Restaging.removeSlides([slide]), undo: undo)
            undo?.setActionName("Delete Slide")
            chosen = next.states.first
            said = "Slide deleted"
        }
    }

    /// The deck played here, as presented, keeping the time each slide takes (PLAN 3.14).
    private func rehearse() {
        showsSlides = false
        rehearsal = Rehearsal(slots: editor.slots)
    }

    /// What a press on an empty slot's words does on `state` (PLAN 3.30): fills it, on a slide made
    /// in this window; a slide the deck opened with outlines no slot, and offers none to fill.
    private func filling(_ state: String) -> ((WaitingSlot) -> Void)? {
        guard fresh.contains(state) else { return nil }
        return { slot in fill(slot, in: state) }
    }

    /// A slot of the layout of `state` that waits, filled as a press on its words asks (PLAN 3.30):
    /// its words put back as a new slide in the layout puts them there, in the slot's role, typed in
    /// with their words selected, so typing replaces them; else a picture, or a sheet's data, asked
    /// for and put there.
    private func fill(_ slot: WaitingSlot, in state: String) {
        pointed = CGPoint(x: slot.rect.midX, y: slot.rect.midY)
        guard slot.typed else {
            choosingPicture = true
            return
        }
        perform {
            let filled = try editor.session.filling(state: state, slot: slot.slot)
            try document.make(filled.patch, undo: undo)
            node = filled.id
            also = []
            if let typing, typing.enter(filled.id, in: state, at: nil) {
                typing.selectAll()
            }
        }
    }

    /// Insert what the deck offers `n`th in the slide shown.
    private func insertHere(_ n: Int) {
        if let shown { insert(n, in: shown) }
    }

    /// `view`, keeping up with the window: the state shown, the undo the assistant's edits go
    /// into, and every state linted once edits stop.
    private func watched(_ view: some View) -> some View {
        view
            .onChange(of: shown, initial: true) { _, now in
                // The slide shown is the one the list selects: the deck's first, as it opens.
                if chosen != now { chosen = now }
                // A state chosen plays its cue, as it does in the browser.
                editor.shown = now
                node = arriving
                also = []
                arriving = nil
                playhead = Playhead()
                typing?.sync(shown: now)
            }
            .task(id: said) {
                // What the canvas says of an edit, a moment over the slide, then gone.
                guard said != nil else { return }
                try? await Task.sleep(for: .seconds(6))
                if !Task.isCancelled { said = nil }
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
            .task(id: "\(shown ?? "")\u{1f}\(selection.joined(separator: ","))\u{1f}\(editor.revision)") {
                // What the Node menu offers, read again as the deck or the selection changes.
                inserts = (try? editor.session.inserts()) ?? []
                let boxes = shown.flatMap { try? editor.session.boxes(state: $0) } ?? []
                ownLocks = Set(boxes.filter { $0.locked == $0.node }.map(\.node))
                if let shown, let node, let choices = try? editor.session.choices(state: shown, node: node) {
                    grouped = choices.type == "group"
                } else {
                    grouped = false
                }
                elsewhere = shown.map { showsElsewhere(in: $0) } ?? false
            }
            .task(id: said) {
                // What the canvas says of an edit fades once read, as a presentation app's tips do.
                guard said != nil else { return }
                try? await Task.sleep(for: .seconds(5))
                if !Task.isCancelled { said = nil }
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
            .sheet(isPresented: $remoting) {
                RemoteSheet(editor: editor) { shown }
            }
            .sheet(item: $asked) { asked in
                OfferSheet(title: asked.title, ask: asked.ask, take: asked.take) { ops in
                    perform { try document.make(ops, undo: undo) }
                }
            }
            .sheet(isPresented: choosingTheme) {
                ThemeChooser(themes: offeredThemes) { name in
                    offeredThemes = []
                    if let name { begin(in: name) }
                }
            }
            // A deck New made: the themes that ship offered first, once its window is up.
            .task {
                guard document.startsNew else { return }
                document.startsNew = false
                // Its title slide's empty places outlined, as on a slide made here.
                fresh.formUnion(editor.slots.map(\.state))
                try? await Task.sleep(for: .milliseconds(250))
                offeredThemes = (try? editor.session.shippedThemes()) ?? []
            }
            .alert("Couldn’t Make That Change", isPresented: failing, presenting: failure) { _ in
                Button("OK") { failure = nil }
            } message: { said in
                Text(said)
            }
    }

    /// Whether an export waits to be saved.
    private var exporting: Binding<Bool> {
        Binding(get: { saving != nil }, set: { if !$0 { saving = nil } })
    }

    /// Whether the themes that ship are offered.
    private var choosingTheme: Binding<Bool> {
        Binding(get: { !offeredThemes.isEmpty }, set: { if !$0 { offeredThemes = [] } })
    }

    /// A deck New made put in the theme that ships named `name`, before anything is on it (PLAN
    /// 3.3): no step to undo. What only the theme it leaves named, its file and its fonts, goes
    /// with it, so the bundle holds the one theme.
    private func begin(in name: String) {
        perform {
            let session = editor.session
            let ships = try session.shippedThemes().first { $0.name == name }
            let current = try session.themes().current.map { ($0 as NSString).lastPathComponent }
            guard let ships, current != ships.file else { return }
            let themed = try session.retheme(ships: name)
            guard themed.applied else {
                throw Refused(description: "not re-themed: \(themed.why.first ?? "the deck would not validate in it")")
            }
            for file in try session.bundleFiles() where file.named.isEmpty {
                try session.removeFile(file.path)
            }
            editor.reread()
        }
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
            // The slide fitted to the room the window gives it, which it does not ask the window
            // for: the window opens at its own size, within the screen, whatever the slide's shape.
            GeometryReader { room in
                let fitted = Self.fitted(size, in: room.size, margin: 28)
                // Drawn again after each edit, and as a drag moves a node or shows its patch (PLAN 3.7).
                ScaenaCanvas(
                    session: editor.session, state: shown, revision: editor.revision &+ editor.drawn,
                    playhead: $playhead
                )
                .overlay {
                    if showsGrid { GridOverlay(editor: editor, shown: zoom.view) }
                }
                .overlay {
                    // The guides that hold nothing off the slide, and the layout's empty slots.
                    ZStack {
                        if showsSafeArea { SafeAreaOverlay(editor: editor, shown: zoom.view) }
                        if !showsSlides, fresh.contains(shown) {
                            WaitingOverlay(editor: editor, state: shown, shown: zoom.view)
                        }
                    }
                }
                .overlay {
                    if let typing {
                        CanvasSelection(
                            editor: editor, state: shown, size: size, zoom: $zoom, node: $node, also: $also,
                            typing: typing, pointed: $pointed, said: $said, delete: delete, clip: clip,
                            finding: { searching = true }, actions: actions,
                            fix: { finding in perform { try document.fix(finding, undo: undo) } },
                            dropped: { items, at in drop(items, in: shown, at: at) },
                            fill: filling(shown)
                        ) { ops in
                            perform { try document.make(ops, undo: undo) }
                        }
                    }
                }
                // The whole canvas shown again once it is another size: another format shown, say.
                .onChange(of: size, initial: true) { _, now in zoom.resize(to: now) }
                .onChange(of: zoom) { _, now in editor.look(through: now.view) }
                .frame(width: fitted.width, height: fitted.height)
                // The slide on the gray a slide sits on, lifted off it a little (PLAN 3.18).
                .background {
                    Rectangle()
                        .fill(.background)
                        .shadow(color: .black.opacity(0.2), radius: 10, y: 3)
                }
                .position(x: room.size.width / 2, y: room.size.height / 2)
            }
            .frame(minWidth: 320, minHeight: 200)
            .background(Desk.color)
        } else if let why = editor.unshown {
            // The deck opened, but its source is no deck yet: why, and where to mend it.
            ContentUnavailableView {
                Label("This Deck Can’t Be Shown", systemImage: "exclamationmark.triangle")
            } description: {
                Text(why)
            } actions: {
                Button("Show Source") { showsSource = true }
                    .disabled(showsSource)
            }
        } else {
            ContentUnavailableView("No Slides", systemImage: "rectangle.stack")
        }
    }

    /// The largest box of `canvas`'s shape in `room`, `margin` points in from each of its edges.
    private static func fitted(_ canvas: CGSize, in room: CGSize, margin: CGFloat) -> CGSize {
        let (w, h) = (max(room.width - 2 * margin, 1), max(room.height - 2 * margin, 1))
        let shape = canvas.width / max(canvas.height, 1)
        return w / h > shape ? CGSize(width: h * shape, height: h) : CGSize(width: w, height: w / shape)
    }

    /// Show a match the find bar found (PLAN 3.16): its state, and its node selected.
    private func reveal(_ state: String?, _ found: String?) {
        if let state, state != shown {
            arriving = found
            chosen = state
        } else {
            node = found
            also = []
        }
    }

    /// Lay the canvas out in `format`, or on the deck's own canvas (PLAN 3.16, as the browser's
    /// formats, PLAN 2.62): the canvas, its boxes, its grid, and what lint says holds there follow,
    /// and a move of a node with a layout of its own there moves it there alone (PLAN 2.85).
    private func showFormat(_ format: String?) {
        guard format != editor.format else { return }
        perform { try editor.show(format: format) }
        said = format.map { "Showing the slide in \($0)" } ?? "Showing the slide in its own size"
    }

    /// Give `node` a layout of its own in `format`, the format shown, where it stands now (ADR-0020,
    /// PLAN 2.85): from then on a move there moves it there alone. One step to undo.
    private func placeAnew(_ node: String, in format: String, state: String) {
        perform {
            guard let patch = try editor.session.placingAnew(node, state: state, in: format) else {
                said = "Laid out on its own in \(format) already"
                return
            }
            try document.make(patch, undo: undo)
            said = "Laid out on its own in \(format): a move here moves it here alone · ⌘Z undoes it"
        }
    }

    /// What the View menu does to the canvas (PLAN 3.16): zoom, the whole canvas, and find.
    private var canvasActions: CanvasActions? {
        guard shown != nil, !showsSlides, rehearsal == nil else { return nil }
        return CanvasActions(
            zoom: { by in zoom.step(by) }, fit: { zoom.fit() }, zoomed: zoom.level > 1, find: { searching = true })
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
        asked = Asked(title: "What This Means", ask: { try await OnDevice.explain(finding, text: text) }, take: nil)
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
        var made = DeckActions(
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
        made.choosePicture = { choosingPicture = true }
        made.deletesElsewhere = elsewhere
        // In another of the deck's formats, a node given a layout of its own there (PLAN 2.85).
        if one, let node, let format = editor.format {
            made.placeAnew = { placeAnew(node, in: format, state: shown) }
        }
        return made
    }

    /// The Edit menu's Copy, Cut, and Paste on the canvas (PLAN 3.12).
    private func clip(_ what: Clipping) {
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
            said = cut ? "Nothing selected to cut" : "Nothing selected to copy"
            return
        }
        do {
            Pasteboard.write(clip: try editor.session.copying(state: shown, nodes: nodes))
        } catch {
            said = "Not copied: \(error)"
            return
        }
        said = cut ? "Cut" : "Copied"
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
            landWords(text, in: shown, at: at)
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

    /// Words pasted or dropped in `state` about `at` (PLAN 2.96, 4.7): a sheet's cells as the
    /// source they read as and the table they were; else a clip's text as its nodes, or a text.
    private func landWords(_ text: String, in state: String, at: CGPoint, verb: String = "pasted") {
        if let cells = (try? editor.session.cells(text)) ?? nil {
            attach(Data(cells.csv.utf8), named: "\(cells.name).csv", cells: cells, in: state, at: at, verb: verb)
        } else {
            pasteText(text, in: state, at: at, verb: verb)
        }
    }

    /// A clip's text, or words, pasted in `state` about `at`.
    private func pasteText(_ text: String, in state: String, at: CGPoint, verb: String = "pasted") {
        perform {
            let pasted = try editor.session.pasting(text, state: state, at: at)
            try document.make(pasted.patch, undo: undo)
            node = pasted.id
            also = pasted.also
            let done: String = verb.prefix(1).uppercased() + verb.dropFirst()
            said = ([done] + pasted.lacked).joined(separator: "; ")
        }
    }

    /// A picture kept in the bundle, by its content, and inserted as an image about `at`.
    private func picture(_ data: Data, named name: String, in state: String, at: CGPoint, verb: String = "pasted") {
        perform {
            let path = try editor.session.drop(data, named: name)
            let offered = try editor.session.inserts()
            guard let n = offered.firstIndex(where: { $0.node["type"]?.string == "image" && $0.node["src"]?.string == path })
            else {
                said = "\(name) is not a picture the deck can show: a PNG or a JPEG"
                return
            }
            let added = try editor.session.inserting(state: state, n: n, at: at, named: name)
            try document.make(added.patch, undo: undo)
            node = added.id
            also = []
            said = "Picture \(verb)"
        }
    }

    /// A data file kept in the bundle and declared as a source (PLAN 2.76), then a chart of it
    /// inserted about `at`; a sheet's `cells`, typed as they read, and the table they were, each
    /// column printing its figures as they were copied (PLAN 2.96).
    private func attach(
        _ data: Data, named name: String, cells: Cells?, in state: String, at: CGPoint, verb: String = "pasted"
    ) {
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
                said = "The data is kept as \(attaching.data), but no \(kind) of it can be made"
                return
            }
            let with: JSONValue? = cells.map { ["columns": $0.tableColumns] }
            let added = try editor.session.inserting(state: state, n: n, at: at, with: with)
            try document.make(added.patch, undo: undo)
            node = added.id
            also = []
            said = "A \(kind) of \(attaching.data) \(verb)"
        }
    }

    /// What another app dropped on the canvas at `point`, canvas units (PLAN 3.22), each file as the
    /// browser's canvas takes one (PLAN 2.45, 2.76): a picture let go on an image takes its place,
    /// one `choose` of `src`; anywhere else it is inserted there, as Insert inserts one; a CSV or
    /// JSON file joins the deck as a data source, with a chart of it there. Whether any is taken.
    private func drop(_ items: [NSItemProvider], in state: String, at point: CGPoint) -> Bool {
        let taken = items.filter { item in
            Pasteboard.droppable.contains { item.hasItemConformingToTypeIdentifier($0.identifier) }
        }
        guard !taken.isEmpty else { return false }
        Task { @MainActor in
            for item in taken {
                if let file = await Pasteboard.dropped(item) {
                    land(file.data, named: file.name, in: state, at: point)
                } else if let text = await Pasteboard.droppedWords(item) {
                    // Words land as they paste (PLAN 4.7).
                    landWords(text, in: state, at: point, verb: "added")
                } else {
                    said = "Only a picture, data (CSV, JSON), or words can go on a slide"
                }
            }
        }
        return true
    }

    /// A file dropped at `point`: data as a source with a chart of it; a picture on the image there,
    /// or inserted.
    private func land(_ data: Data, named name: String, in state: String, at point: CGPoint) {
        let ext = (name as NSString).pathExtension.lowercased()
        if ext == "csv" || ext == "json" {
            return attach(data, named: name, cells: nil, in: state, at: point, verb: "added")
        }
        let hit = ((try? editor.session.hits(state: state, at: point)) ?? []).first { $0.locked == nil }
        guard let onto = hit?.node, (try? editor.session.choices(state: state, node: onto))?.type == "image" else {
            return picture(data, named: name, in: state, at: point, verb: "added")
        }
        perform {
            let path = try editor.session.drop(data, named: name)
            let op: JSONValue = [
                "op": "choose", "node": .string(onto), "prop": "src", "value": .string(path), "state": .string(state),
            ]
            try document.make([op], undo: undo)
            node = onto
            also = []
            said = "Picture replaced"
        }
    }

    /// ⌥⌘C: `node`'s look as `state` shows it, copied for ⌥⌘V (PLAN 2.58).
    private func copyLook(_ node: String, in state: String) {
        perform {
            let look = try editor.session.look(state: state, node: node)
            guard look["props"]?.array?.isEmpty == false else {
                said = "No style of its own to copy"
                return
            }
            copiedLook = look
            said = "Style copied: ⌥⌘V pastes it on what is selected"
        }
    }

    /// ⌥⌘V: the look copied pasted on each node selected, in the state shown: one patch of
    /// `choose`s, each written where that node's own value lives (PLAN 2.58).
    private func pasteLook(in state: String) {
        let nodes = selection
        guard let look = copiedLook, !nodes.isEmpty else { return }
        perform {
            let put = try editor.session.putting(state: state, look: look, nodes: nodes)
            let refused = put.refused.map(\.why)
            guard !put.patch.isEmpty else {
                said = put.same.isEmpty ? (refused.first ?? "Nothing here takes that style") : "It has that style already"
                return
            }
            try document.make(put.patch, undo: undo)
            said = (["Style pasted"] + refused).joined(separator: "; ")
        }
    }

    /// Insert what the deck offers `n`th where the pointer last pressed on the canvas, or in its
    /// middle; in the room nearest there where content would overlap (PLAN 2.34, 2.79). It enters
    /// in the state shown, selected: one step to undo. A text enters typed in, its words selected,
    /// so what is typed next takes their place, as a presentation app's new text box does (PLAN 3.21).
    private func insert(_ n: Int, in state: String) {
        let size = (try? editor.session.canvasSize()) ?? CGSize(width: 1920, height: 1080)
        let at = pointed ?? CGPoint(x: size.width / 2, y: size.height / 2)
        let offered = inserts.indices.contains(n) ? inserts[n] : nil
        perform {
            let added = try editor.session.inserting(state: state, n: n, at: at)
            try document.make(added.patch, undo: undo)
            node = added.id
            also = []
            if offered?.kind == "Text", let typing, typing.enter(added.id, in: state, at: nil) {
                typing.selectAll()
                return
            }
            said = "\(offered.map(Words.inserted) ?? "Object") inserted"
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
                said = "Not duplicated: two of the copies would have one name"
                return
            }
            try document.make(added.flatMap(\.patch), undo: undo)
            node = ids.first
            also = Array(ids.dropFirst())
            said = "Duplicated"
        }
    }

    /// Take the nodes selected out of the state shown and the states after it, and out of the
    /// deck where no state shows them after; or, `everywhere`, out of the deck (Delete,
    /// Shift+Delete). Locked nodes are not taken: one patch, one step to undo.
    private func delete(_ everywhere: Bool) {
        let nodes = selection
        guard let shown, !nodes.isEmpty else {
            said = "Nothing selected to delete"
            return
        }
        let boxes = (try? editor.session.boxes(state: shown)) ?? []
        if let held = boxes.first(where: { nodes.contains($0.node) && $0.locked != nil }), let holder = held.locked {
            said = holder == held.node ? "It is locked: ⇧⌘L unlocks it" : "What holds it is locked: unlock that first"
            return
        }
        perform {
            let ops = try nodes.flatMap { try editor.session.deleting(state: shown, node: $0, everywhere: everywhere) }
            let gone = ops.allSatisfy { $0["op"]?.string == "remove_node" }
            try document.make(ops, undo: undo)
            node = nil
            also = []
            said = gone ? "Deleted from every slide · ⌘Z brings it back" : "Deleted · ⌘Z brings it back"
        }
    }

    /// Whether a node selected shows on a slide other than `state`'s: what Delete from All Slides
    /// takes it off too, where Delete takes it off this slide on. Read as that delete would reach.
    private func showsElsewhere(in state: String) -> Bool {
        let nodes = selection
        guard !nodes.isEmpty, let slide = editor.slots.first(where: { $0.state == state })?.slide else {
            return false
        }
        let here = Set(editor.slots.filter { $0.slide == slide }.map(\.state))
        let session = editor.session
        let ops = (try? nodes.flatMap { try session.deleting(state: state, node: $0, everywhere: true) }) ?? []
        guard !ops.isEmpty, let reached = try? session.reach(ops) else { return false }
        return reached.contains { !here.contains($0) }
    }

    /// Lock the nodes selected by their own lock, or unlock them where each is so locked
    /// (⇧⌘L, PLAN 2.95): the canvas passes over a node locked, in every state. One step to undo.
    private func lock() {
        let nodes = selection
        let (ops, locks) = ScaenaKit.locking(nodes, own: { ownLocks.contains($0) })
        guard !ops.isEmpty else { return }
        perform {
            try document.make(ops, undo: undo)
            said = locks ? "Locked: a click passes over it · ⇧⌘L unlocks it" : "Unlocked"
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
            said = "Grouped"
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
            said = "Ungrouped"
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
                said = how == "front" || how == "forward" ? "In front already" : "Behind already"
                return
            }
            try document.make(arranged.patch, undo: undo)
            switch how {
            case "front": said = "Brought to front"
            case "back": said = "Sent to back"
            case "forward": said = "Brought forward"
            default: said = "Sent backward"
            }
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

    /// What the panels do to the deck (PLAN 3.15): each edit one step of the window's undo, named
    /// as the Edit menu says it (PLAN 3.28).
    private var panelEdits: PanelEdits {
        PanelEdits(
            besideMade: { name, edit in making(name) { try document.beside(edit, undo: undo) } },
            dataMade: { name, edit in making(name) { try document.data(edit, undo: undo) } })
    }

    /// `edit` made, its step to undo named `name` in the Edit menu: whether it was. Where it was
    /// not, the window says why, and no step is named.
    private func making(_ name: String, _ edit: () throws -> Void) -> Bool {
        do {
            try edit()
            undo?.setActionName(name)
            return true
        } catch {
            failure = "\(error)"
            return false
        }
    }

    /// The state list's patches (PLAN 3.14): one step to undo, then `then` shown.
    private func restage(_ ops: [JSONValue], then: String?) {
        perform {
            try document.make(ops, undo: undo)
            if let then { chosen = then }
        }
    }

    /// A slide started in `layout` after `state`'s slide, or a blank one with none, then shown
    /// (PLAN 3.30): one step to undo.
    private func startSlide(after state: String, layout: String?) {
        perform {
            let started = try editor.session.starting(after: state, layout: layout)
            fresh.insert(started.id)
            try document.make(started.patch, undo: undo)
            undo?.setActionName("New Slide")
            chosen = started.id
            said = "Slide added"
        }
    }

    /// A step or a slide added after `state`, then shown (PLAN 2.35): one step to undo.
    private func addState(after state: String, as what: StateAdding) {
        perform {
            let added = try editor.session.addingState(after: state, as: what)
            if what == .slide { fresh.insert(added.id) }
            try document.make(added.patch, undo: undo)
            chosen = added.id
            said = what == .step ? "Step added" : "Slide added"
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
            said = "Kept the rehearsal's timing for \(holds.count) slide\(holds.count == 1 ? "" : "s")"
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
