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
/// (⌥⌘P) and Rehearse Slideshow (⌥⌘R).
struct WindowCommands: Commands {
    @FocusedValue(\.panes) private var panes

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
        }
    }

    /// Whether the window shows a part of it, as a menu's check mark; off with no window.
    private func shows(_ pane: KeyPath<WindowPanes, Binding<Bool>>) -> Binding<Bool> {
        panes?[keyPath: pane] ?? .constant(false)
    }
}

/// A kind of thing to insert, as the toolbar offers it (PLAN 3.18): a click inserts the one the
/// kind starts with (`first`, else what the deck offers first), and its menu lists each the deck
/// offers of its kinds, in the order it offers them, landing where the pointer last pressed on the
/// canvas.
struct InsertMenu: View {
    let title: String
    let symbol: String
    /// The kinds it lists, as `Insert.kind` names them: a divider between each.
    let kinds: [String]
    /// The name of the one a click inserts.
    let first: String?
    let inserts: [Insert]
    /// Insert what the deck offers `n`th.
    let insert: (Int) -> Void

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
        Menu {
            ForEach(kinds.indices, id: \.self) { k in
                if k > 0, !offered(kinds[k]).isEmpty { Divider() }
                ForEach(offered(kinds[k])) { item in
                    Button(Words.insertion(item.insert, among: inserts)) { insert(item.id) }
                }
            }
        } label: {
            Label(title, systemImage: symbol)
        } primaryAction: {
            if let one = all.first(where: { $0.insert.name == first }) ?? all.first { insert(one.id) }
        }
        .disabled(all.isEmpty)
        .help(all.isEmpty ? "Nothing of this kind to insert" : "Insert \(title.lowercased()); its menu offers each kind")
        .accessibilityIdentifier("insert-\(title)")
    }
}
