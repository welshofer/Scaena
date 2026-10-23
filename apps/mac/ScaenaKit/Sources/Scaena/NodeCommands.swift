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
    /// Paste the look copied on the nodes selected.
    let pasteLook: (() -> Void)?
    /// Put the nodes selected in a new group (PLAN 2.43).
    let group: (() -> Void)?
    /// Take the group selected apart.
    let ungroup: (() -> Void)?
    /// Order the nodes selected among what their container paints: `forward`, `backward`,
    /// `front`, or `back` (PLAN 2.42).
    let order: ((String) -> Void)?
    /// In another of the deck's formats, give the node selected a layout of its own there, where
    /// it stands (PLAN 2.85, 3.16).
    var placeAnew: (() -> Void)? = nil
    /// Ask for a picture, or a sheet's data, from a file, to put on the slide shown.
    var choosePicture: (() -> Void)? = nil
    /// Offer the icons that ship, to put one on the slide shown (PLAN 2.104).
    var chooseIcon: (() -> Void)? = nil
    /// Make a picture by the user's image model for the image selected, the slide's empty place
    /// for one, or where the pointer last pressed (PLAN 2.107).
    var makePicture: (() -> Void)? = nil
    /// Insert › Map… (PLAN O.1d): a map of a place, in the image selected, the slide's empty place
    /// for a picture, or where the pointer last pressed.
    var insertMap: (() -> Void)? = nil
    /// Join the first object selected to the second with an arrow, `true`, or a line, which
    /// follows them (PLAN 2.109).
    var connect: ((Bool) -> Void)? = nil
    /// Draw the text selected as a diagram of a kind in its place (PLAN 2.110).
    var diagram: ((DiagramKind) -> Void)? = nil
    /// Take up the pen, or put it down (PLAN 2.114), and whether it is taken up.
    var draw: (() -> Void)? = nil
    var drawing = false
    /// Copy the words of the code selected as text (PLAN 2.133).
    var copyCode: (() -> Void)? = nil
    /// Paste the words on the pasteboard as words, never as code (PLAN 2.133).
    var pasteAsText: (() -> Void)? = nil
    /// Whether a node selected shows on another slide too: Delete from All Slides takes it off
    /// those as well, where Delete takes it off this slide on.
    var deletesElsewhere = false
    /// The deck's data sources and their files: what Insert names a chart's or a table's data by
    /// (PLAN 3.18).
    var sources: [DataSource] = []
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

/// What is done to objects, in the menus a presentation app has them in (PLAN 3.11, 3.18), as the
/// browser's Insert menu and keys (PLAN 2.34, 2.95): the Insert menu, by kind, what the theme and
/// the bundle offer, landing where the pointer last pressed on the canvas; in the Edit menu,
/// Duplicate (⌘D), Delete, from this slide on, Delete from All Slides, and Copy Style (⌥⌘C) and
/// Paste Style (⌥⌘V, PLAN 2.58); and the Arrange menu: the order among what their container paints
/// (⌥⌘], ⌘], ⌘[, ⌥⌘[, PLAN 2.42), Group (⌥⌘G) and Ungroup (⇧⌥⌘G, PLAN 2.43, Keynote's keys, since
/// ⌘G is the Mac's Find Next, lift-up USE-5), Connect with Arrow and with Line (PLAN 2.109), and
/// Lock (⇧⌘L); and Insert's Diagram from Text, the text selected drawn as a process, a cycle, or a
/// hierarchy (PLAN 2.110). Each acts on every object selected (PLAN 3.13). The canvas takes Delete
/// and Shift+Delete itself, as the browser's does. Between Insert and Arrange, as a presentation
/// app has it, the Slide menu: New Slide (⇧⌘N), New Step, Duplicate Slide, and Delete Slide, on the
/// slide shown; and the Format menu, on the text typed in: Bold (⌘B), Italic (⌘I), Add Link (⌘K),
/// and the lists, which the canvas takes itself while a text is typed in and the menu names, so
/// Help's search finds them. Each key is `MenuKey`'s, the iPad's canvas's too.
struct NodeCommands: Commands {
    @FocusedValue(\.deck) private var deck
    @FocusedValue(\.panes) private var panes
    @FocusedValue(\.canvasActions) private var canvas

    var body: some Commands {
        CommandMenu("Insert") {
            ForEach(kinds, id: \.self) { kind in
                Menu(Self.kind(kind)) {
                    ForEach(offered(of: kind)) { item in
                        Button(item.name) { deck?.insert(item.id) }
                    }
                }
            }
            if kinds.isEmpty {
                Button("Nothing to Insert") {}
                    .disabled(true)
            }
            Divider()
            Button("Icon…") { deck?.chooseIcon?() }
                .disabled(deck?.chooseIcon == nil)
            Button("Choose Picture…") { deck?.choosePicture?() }
                .disabled(deck?.choosePicture == nil)
            Button("Make a Picture…") { deck?.makePicture?() }
                .disabled(deck?.makePicture == nil)
            Button("Map…") { deck?.insertMap?() }
                .disabled(deck?.insertMap == nil)
            // The pen (PLAN 2.114): each drag on the slide a stroke, the strokes one drawing.
            Button(deck?.drawing == true ? "Stop Drawing" : "Drawing") { deck?.draw?() }
                .disabled(deck?.draw == nil)
            // The text selected drawn as a diagram in its place (PLAN 2.110).
            Menu("Diagram from Text") {
                ForEach(DiagramKind.allCases, id: \.self) { kind in
                    Button(kind.title) { deck?.diagram?(kind) }
                }
            }
            .disabled(deck?.diagram == nil)
        }
        CommandMenu("Slide") {
            Button("New Slide") { panes?.newSlide?() }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                .disabled(panes?.newSlide == nil)
            Button("New Step") { panes?.newStep?() }
                .disabled(panes?.newStep == nil)
            Divider()
            Button("Duplicate Slide") { panes?.duplicateSlide?() }
                .disabled(panes?.duplicateSlide == nil)
            Button("Delete Slide") { panes?.deleteSlide?() }
                .disabled(panes?.deleteSlide == nil)
        }
        CommandMenu("Format") {
            Button("Bold") { canvas?.bold?() }
                .keyboardShortcut(MenuKey.bold.key, modifiers: MenuKey.bold.modifiers)
                .disabled(canvas?.bold == nil)
            Button("Italic") { canvas?.italic?() }
                .keyboardShortcut(MenuKey.italic.key, modifiers: MenuKey.italic.modifiers)
                .disabled(canvas?.italic == nil)
            Divider()
            Button("Add Link…") { canvas?.link?() }
                .keyboardShortcut(MenuKey.link.key, modifiers: MenuKey.link.modifiers)
                .disabled(canvas?.link == nil)
            Divider()
            Button("Bulleted List") { canvas?.list?("bullet") }
                .disabled(canvas?.list == nil)
            Button("Numbered List") { canvas?.list?("number") }
                .disabled(canvas?.list == nil)
        }
        CommandGroup(after: .pasteboard) {
            Button("Paste as Text") { deck?.pasteAsText?() }
                .disabled(deck?.pasteAsText == nil)
            Button("Copy Code") { deck?.copyCode?() }
                .disabled(deck?.copyCode == nil)
            Divider()
            Button("Duplicate") { deck?.duplicate?() }
                .keyboardShortcut(MenuKey.duplicate.key, modifiers: MenuKey.duplicate.modifiers)
                .disabled(deck?.duplicate == nil)
            Button("Delete from This Slide On") { deck?.delete?(false) }
                .disabled(deck?.delete == nil)
            Button("Delete from All Slides") { deck?.delete?(true) }
                .disabled(deck?.delete == nil)
            Divider()
            Button("Copy Style") { deck?.copyLook?() }
                .keyboardShortcut(MenuKey.copyLook.key, modifiers: MenuKey.copyLook.modifiers)
                .disabled(deck?.copyLook == nil)
            Button("Paste Style") { deck?.pasteLook?() }
                .keyboardShortcut(MenuKey.pasteLook.key, modifiers: MenuKey.pasteLook.modifiers)
                .disabled(deck?.pasteLook == nil)
        }
        CommandMenu("Arrange") {
            Button("Bring to Front") { deck?.order?("front") }
                .keyboardShortcut(MenuKey.bringToFront.key, modifiers: MenuKey.bringToFront.modifiers)
                .disabled(deck?.order == nil)
            Button("Bring Forward") { deck?.order?("forward") }
                .keyboardShortcut(MenuKey.bringForward.key, modifiers: MenuKey.bringForward.modifiers)
                .disabled(deck?.order == nil)
            Button("Send Backward") { deck?.order?("backward") }
                .keyboardShortcut(MenuKey.sendBackward.key, modifiers: MenuKey.sendBackward.modifiers)
                .disabled(deck?.order == nil)
            Button("Send to Back") { deck?.order?("back") }
                .keyboardShortcut(MenuKey.sendToBack.key, modifiers: MenuKey.sendToBack.modifiers)
                .disabled(deck?.order == nil)
            Divider()
            Button("Group") { deck?.group?() }
                .keyboardShortcut(MenuKey.group.key, modifiers: MenuKey.group.modifiers)
                .disabled(deck?.group == nil)
            Button("Ungroup") { deck?.ungroup?() }
                .keyboardShortcut(MenuKey.ungroup.key, modifiers: MenuKey.ungroup.modifiers)
                .disabled(deck?.ungroup == nil)
            Button("Connect with Arrow") { deck?.connect?(true) }
                .disabled(deck?.connect == nil)
            Button("Connect with Line") { deck?.connect?(false) }
                .disabled(deck?.connect == nil)
            Divider()
            Button(deck?.locked == true ? "Unlock" : "Lock") { deck?.lock?() }
                .keyboardShortcut(MenuKey.lock.key, modifiers: MenuKey.lock.modifiers)
                .disabled(deck?.lock == nil)
            if deck?.placeAnew != nil {
                Divider()
                Button("Lay Out on Its Own in This Size") { deck?.placeAnew?() }
            }
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

    /// A kind as the Insert menu names it.
    static func kind(_ kind: String) -> String {
        switch kind {
        case "Image": "Picture"
        case "Shader": "Background Effect"
        default: kind
        }
    }

    /// What is offered of `kind`, each by its place in the offer.
    private func offered(of kind: String) -> [Offered] {
        let all = deck?.inserts ?? []
        return all.enumerated().filter { $0.element.kind == kind }.map {
            Offered(id: $0.offset, name: Words.insertion($0.element, among: all, sources: deck?.sources ?? []))
        }
    }
}

/// One thing offered, by its place in what the deck offers.
private struct Offered: Identifiable {
    let id: Int
    let name: String
}
