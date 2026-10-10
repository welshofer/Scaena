import ScaenaKit
import SwiftUI

/// What the View and Play menus do in the window in front (PLAN 3.18): each part of the window a
/// person may show or put away, the inspector's tab, and the deck played or rehearsed.
struct WindowPanes {
    let slides: Binding<Bool>
    let timeline: Binding<Bool>
    let formats: Binding<Bool>
    let grid: Binding<Bool>
    let issues: Binding<Bool>
    let source: Binding<Bool>
    let assistant: Binding<Bool>
    let inspector: Binding<Bool>
    let tab: Binding<InspectorTab>
    /// Play the deck from the slide shown; none where it has no slides.
    let play: (() -> Void)?
    /// Rehearse it here; none while it cannot be.
    let rehearse: (() -> Void)?
    /// Remote…: let a remote play it (PLAN 4.10).
    let remote: () -> Void
}

private struct WindowPanesKey: FocusedValueKey {
    typealias Value = WindowPanes
}

extension FocusedValues {
    /// What the View and Play menus do in the window in front.
    var panes: WindowPanes? {
        get { self[WindowPanesKey.self] }
        set { self[WindowPanesKey.self] = newValue }
    }
}

/// The View menu's parts of the window and the Play menu (PLAN 3.18), as a presentation app's: the
/// light table (⌥⌘L); the timeline, the other sizes, the grid (⌘'), the issues, and the source,
/// each off until asked for; the assistant (⌥⌘A) and the inspector's tabs; then Play Slideshow
/// (⌥⌘P) and Rehearse Slideshow (⌥⌘R); Remote…, which lets a remote play the deck, and on the
/// iPad, Control a Presentation…, the remote itself (PLAN 4.10).
struct WindowCommands: Commands {
    @FocusedValue(\.panes) private var panes
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(before: .toolbar) {
            Toggle("Light Table", isOn: shows(\.slides))
                .keyboardShortcut("l", modifiers: [.option, .command])
                .disabled(panes == nil)
            Divider()
            Toggle("Show Timeline", isOn: shows(\.timeline))
                .disabled(panes == nil)
            Toggle("Show Other Sizes", isOn: shows(\.formats))
                .disabled(panes == nil)
            Toggle("Show Grid", isOn: shows(\.grid))
                .keyboardShortcut("'", modifiers: .command)
                .disabled(panes == nil)
            Toggle("Show Issues", isOn: shows(\.issues))
                .disabled(panes == nil)
            Toggle("Show Source", isOn: shows(\.source))
                .disabled(panes == nil)
            Divider()
            Toggle("Show Assistant", isOn: shows(\.assistant))
                .keyboardShortcut("a", modifiers: [.option, .command])
                .disabled(panes == nil)
            Toggle("Show Inspector", isOn: shows(\.inspector))
                .keyboardShortcut("i", modifiers: [.option, .command])
                .disabled(panes == nil)
            ForEach(InspectorTab.allCases) { tab in
                Button(tab.title) {
                    panes?.tab.wrappedValue = tab
                    panes?.inspector.wrappedValue = true
                }
                .disabled(panes == nil)
            }
            Divider()
        }
        CommandMenu("Play") {
            Button("Play Slideshow") { panes?.play?() }
                .keyboardShortcut("p", modifiers: [.option, .command])
                .disabled(panes?.play == nil)
            Button("Rehearse Slideshow") { panes?.rehearse?() }
                .keyboardShortcut("r", modifiers: [.option, .command])
                .disabled(panes?.rehearse == nil)
            Divider()
            Button("Remote…") { panes?.remote() }
                .disabled(panes == nil)
            #if !os(macOS)
            Button("Control a Presentation…") { openWindow(id: RemoteWindow.id) }
            #endif
        }
    }

    /// Whether the window shows a part of it, as a menu's check mark; off with no window.
    private func shows(_ pane: KeyPath<WindowPanes, Binding<Bool>>) -> Binding<Bool> {
        panes?[keyPath: pane] ?? .constant(false)
    }
}

/// The toolbar's button that inserts a kind of thing, as a presentation app's (PLAN 3.18): a text
/// at a click, in the theme's body style, the others each offered in its menu, a click away; a
/// divider between each kind it lists. No chevron widens it, so the toolbar keeps every button in
/// view (PLAN 3.27). The Insert menu offers the same, each text style among them.
struct InsertMenu: View {
    let title: String
    let symbol: String
    /// The kinds it lists, as `Insert.kind` names them: a divider between each.
    let kinds: [String]
    /// The name of the one a click inserts; none where a click opens the menu.
    let first: String?
    let inserts: [Insert]
    /// Insert what the deck offers `n`th.
    let insert: (Int) -> Void
    /// Choose a file to insert, by what the menu calls it, offered last; none where there is no
    /// slide to put it on.
    var choose: (title: String, action: () -> Void)? = nil

    /// One thing offered, by its place in what the deck offers.
    private struct Offered: Identifiable {
        let id: Int
        let insert: Insert
    }

    private func offered(_ kind: String) -> [Offered] {
        inserts.enumerated().filter { $0.element.kind == kind }.map { Offered(id: $0.offset, insert: $0.element) }
    }

    var body: some View {
        let all = kinds.flatMap(offered)
        menu(all)
            .menuIndicator(.hidden)
            .disabled(all.isEmpty && choose == nil)
            .help(help(all))
            .accessibilityIdentifier("insert-\(title)")
    }

    /// What it offers: each thing of its kinds, then Choose….
    @ViewBuilder private func items() -> some View {
        ForEach(kinds.indices, id: \.self) { k in
            if k > 0, !offered(kinds[k]).isEmpty { Divider() }
            ForEach(offered(kinds[k])) { item in
                Button(Words.insertion(item.insert, among: inserts)) { insert(item.id) }
            }
        }
        if let choose {
            if !kinds.flatMap(offered).isEmpty { Divider() }
            Button(choose.title, action: choose.action)
        }
    }

    /// A menu a click opens, or, with a first, one a click inserts it from and a press holds open.
    @ViewBuilder private func menu(_ all: [Offered]) -> some View {
        if let first {
            Menu {
                items()
            } label: {
                Label(title, systemImage: symbol)
            } primaryAction: {
                if let one = all.first(where: { $0.insert.name == first }) ?? all.first { insert(one.id) }
            }
        } else {
            Menu {
                items()
            } label: {
                Label(title, systemImage: symbol)
            }
        }
    }

    private func help(_ all: [Offered]) -> String {
        if all.isEmpty, choose == nil { return "Nothing of this kind to insert" }
        return first == nil ? "Insert \(title.lowercased())" : "Insert \(title.lowercased()); hold for each style"
    }
}

#if !os(macOS)
/// Undo on the iPad's toolbar, as a presentation app's there (PLAN 4.11): a tap takes back the last
/// step, and a long press offers Redo. An iPad without a keyboard has no ⌘Z, and its menu bar is a
/// swipe from the top away. It reads as the step it takes back, as the Edit menu would name it:
/// Undo Body Size.
struct UndoButton: View {
    let undo: UndoManager?
    /// What a tap takes back and what Redo makes again, as the window's undo names them.
    @State private var steps = UndoSteps()

    var body: some View {
        Menu {
            Button {
                if undo?.canRedo == true { undo?.redo() }
            } label: {
                Label(steps.redo ?? "Redo", systemImage: "arrow.uturn.forward")
            }
            .disabled(steps.redo == nil)
        } label: {
            Label("Undo", systemImage: "arrow.uturn.backward")
        } primaryAction: {
            if undo?.canUndo == true { undo?.undo() }
        }
        .disabled(steps.undo == nil && steps.redo == nil)
        .help(steps.undo ?? "Nothing to undo; its menu offers Redo")
        .accessibilityLabel(steps.undo ?? "Undo")
        .accessibilityIdentifier("undo")
        .onAppear { steps = UndoSteps(undo) }
        .onChange(of: undo) { _, now in steps = UndoSteps(now) }
        .onReceive(NotificationCenter.default.publisher(for: .NSUndoManagerDidCloseUndoGroup), perform: read)
        .onReceive(NotificationCenter.default.publisher(for: .NSUndoManagerDidUndoChange), perform: read)
        .onReceive(NotificationCenter.default.publisher(for: .NSUndoManagerDidRedoChange), perform: read)
    }

    /// The steps read again when the window's undo makes, takes back, or makes again one.
    private func read(_ note: Notification) {
        guard let undo, (note.object as AnyObject?) === undo else { return }
        steps = UndoSteps(undo)
    }
}

/// What an undo manager would take back and make again, by the titles its menu items would have:
/// none where there is nothing to.
struct UndoSteps: Equatable {
    var undo: String?
    var redo: String?

    init(_ manager: UndoManager? = nil) {
        undo = manager?.canUndo == true ? manager?.undoMenuItemTitle : nil
        redo = manager?.canRedo == true ? manager?.redoMenuItemTitle : nil
    }
}
#endif
