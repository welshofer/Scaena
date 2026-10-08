import ScaenaKit
import SwiftUI

/// What the Node menu does in the window in front (PLAN 3.11): what the deck may have inserted,
/// and what is done to the node selected in the state shown. A part it cannot do now is none.
struct DeckActions {
    let inserts: [Insert]
    /// Insert what `inserts` offers `n`th.
    let insert: (Int) -> Void
    /// A copy of the node selected beside it.
    let duplicate: (() -> Void)?
    /// Take the node selected away: from the state shown on, or, `true`, from the deck.
    let delete: ((Bool) -> Void)?
    /// Lock the node selected, or unlock it.
    let lock: (() -> Void)?
    /// Whether the node selected is locked by its own lock.
    let locked: Bool
    /// Copy the look of the node selected (PLAN 2.58).
    let copyLook: (() -> Void)?
    /// Paste the look copied on the node selected.
    let pasteLook: (() -> Void)?
}

private struct DeckActionsKey: FocusedValueKey {
    typealias Value = DeckActions
}

extension FocusedValues {
    /// What the Node menu does in the window in front.
    var deck: DeckActions? {
        get { self[DeckActionsKey.self] }
        set { self[DeckActionsKey.self] = newValue }
    }
}

/// The Node menu (PLAN 3.11), as the browser's Insert menu and keys (PLAN 2.34, 2.95): Insert, by
/// kind, what the theme and the bundle offer, landing where the pointer last pressed on the canvas;
/// Duplicate (⌘D); Delete, from the state shown on, and from the deck; Copy Look (⌥⌘C) and Paste
/// Look (⌥⌘V, PLAN 2.58); and Lock (⇧⌘L). The canvas takes Delete and Shift+Delete itself, as the
/// browser's does.
struct NodeCommands: Commands {
    @FocusedValue(\.deck) private var deck

    var body: some Commands {
        CommandMenu("Node") {
            Menu("Insert") {
                ForEach(kinds, id: \.self) { kind in
                    Menu(kind) {
                        ForEach(offered(of: kind)) { item in
                            Button(item.name) { deck?.insert(item.id) }
                        }
                    }
                }
            }
            .disabled(kinds.isEmpty)
            Divider()
            Button("Duplicate") { deck?.duplicate?() }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(deck?.duplicate == nil)
            Button("Delete from Here On") { deck?.delete?(false) }
                .disabled(deck?.delete == nil)
            Button("Delete from the Deck") { deck?.delete?(true) }
                .disabled(deck?.delete == nil)
            Divider()
            Button("Copy Look") { deck?.copyLook?() }
                .keyboardShortcut("c", modifiers: [.command, .option])
                .disabled(deck?.copyLook == nil)
            Button("Paste Look") { deck?.pasteLook?() }
                .keyboardShortcut("v", modifiers: [.command, .option])
                .disabled(deck?.pasteLook == nil)
            Divider()
            Button(deck?.locked == true ? "Unlock" : "Lock") { deck?.lock?() }
                .keyboardShortcut("l", modifiers: [.command, .shift])
                .disabled(deck?.lock == nil)
        }
    }

    /// The kinds of what is offered, in the order the deck offers them.
    private var kinds: [String] {
        var seen: [String] = []
        for insert in deck?.inserts ?? [] where !seen.contains(insert.kind) {
            seen.append(insert.kind)
        }
        return seen
    }

    /// What is offered of `kind`, each by its place in the offer.
    private func offered(of kind: String) -> [Offered] {
        (deck?.inserts ?? []).enumerated().filter { $0.element.kind == kind }.map { Offered(id: $0.offset, name: $0.element.name) }
    }
}

/// One thing offered, by its place in what the deck offers.
private struct Offered: Identifiable {
    let id: Int
    let name: String
}
