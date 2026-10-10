import ScaenaKit
import SwiftUI

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// One thing the canvas's menu offers (PLAN 3.23, 4.3): the iPad's long press and right click and
/// the Mac's right click offer the same, each as its platform shows a menu.
enum CanvasOffer {
    case action(String, symbol: String?, destructive: Bool = false, run: () -> Void)
    case menu(String, symbol: String?, [CanvasOffer])
    /// A line between kinds of things, where the platform draws one.
    case divider

    /// What the menu offers: what the clipboard and the Node menu can do now, and nothing else
    /// (PLAN 3.11, 3.12).
    static func offers(_ deck: DeckActions?, clip: @escaping (Clipping) -> Void) -> [CanvasOffer] {
        var offers: [CanvasOffer] = []
        if deck?.duplicate != nil {
            offers.append(.action("Cut", symbol: "scissors", run: { clip(.cut) }))
            offers.append(.action("Copy", symbol: "doc.on.doc", run: { clip(.copy) }))
        }
        offers.append(.action("Paste", symbol: "doc.on.clipboard", run: { clip(.paste) }))
        guard let deck else { return offers }
        offers.append(.divider)
        if let duplicate = deck.duplicate {
            offers.append(.action("Duplicate", symbol: "plus.square.on.square", run: duplicate))
        }
        if let delete = deck.delete {
            // Delete as the Delete key does; the deck's other slides only where it shows on them.
            offers.append(.action("Delete", symbol: "trash", destructive: true, run: { delete(false) }))
            if deck.deletesElsewhere {
                offers.append(
                    .action("Delete from All Slides", symbol: "trash", destructive: true, run: { delete(true) }))
            }
        }
        var arranging: [CanvasOffer] = []
        if let order = deck.order {
            arranging += [
                .action("Bring to Front", symbol: "square.3.layers.3d.top.filled", run: { order("front") }),
                .action("Bring Forward", symbol: "square.2.layers.3d.top.filled", run: { order("forward") }),
                .action("Send Backward", symbol: "square.2.layers.3d.bottom.filled", run: { order("backward") }),
                .action("Send to Back", symbol: "square.3.layers.3d.bottom.filled", run: { order("back") }),
            ]
        }
        if let group = deck.group { arranging.append(.action("Group", symbol: "square.on.square.dashed", run: group)) }
        if let ungroup = deck.ungroup { arranging.append(.action("Ungroup", symbol: "square.on.square", run: ungroup)) }
        if !arranging.isEmpty { offers.append(.menu("Arrange", symbol: "square.3.layers.3d", arranging)) }
        if let copyLook = deck.copyLook { offers.append(.action("Copy Style", symbol: "paintbrush", run: copyLook)) }
        if let pasteLook = deck.pasteLook {
            offers.append(.action("Paste Style", symbol: "paintbrush.pointed", run: pasteLook))
        }
        if let lock = deck.lock {
            offers.append(.action(deck.locked ? "Unlock" : "Lock", symbol: deck.locked ? "lock.open" : "lock", run: lock))
        }
        if let placeAnew = deck.placeAnew {
            offers.append(.action("Lay Out on Its Own in This Size", symbol: "rectangle.badge.plus", run: placeAnew))
        }
        // What the theme and the bundle offer, by kind, landing where the menu was asked for.
        var kinds: [String] = []
        for insert in deck.inserts where !kinds.contains(insert.kind) { kinds.append(insert.kind) }
        if !kinds.isEmpty {
            let offered = deck.inserts.enumerated().map { (id: $0.offset, insert: $0.element) }
            offers.append(.divider)
            offers.append(
                .menu(
                    "Insert", symbol: "plus",
                    kinds.map { kind in
                        .menu(
                            NodeCommands.kind(kind), symbol: nil,
                            offered.filter { $0.insert.kind == kind }.map { o in
                                .action(
                                    Words.insertion(o.insert, among: deck.inserts), symbol: nil,
                                    run: { deck.insert(o.id) })
                            })
                    }))
        }
        return offers
    }
}

#if os(macOS)
/// A right click on the canvas, or a click with Control (PLAN 3.23), read before any view takes
/// it, as the wheel is: where it clicked, view points. It is taken where `clicked` says so, and
/// otherwise goes on to what is under the pointer.
struct CanvasRightClick: NSViewRepresentable {
    let clicked: (CGPoint) -> Bool

    func makeNSView(context: Context) -> ClickView {
        let view = ClickView()
        view.clicked = clicked
        return view
    }

    func updateNSView(_ view: ClickView, context: Context) {
        view.clicked = clicked
    }

    final class ClickView: NSView {
        var clicked: ((CGPoint) -> Bool)?
        /// Removed as the view leaves its window, or goes.
        nonisolated(unsafe) private var monitor: Any?

        override var isFlipped: Bool { true }

        /// Presses go through to the canvas under it.
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseDown, .leftMouseDown]) { [weak self] event in
                guard let self else { return event }
                return self.take(event)
            }
        }

        /// `event`, a right click over the canvas or a click with Control, taken: none where the
        /// canvas took it, else the event, for what is under the pointer.
        private func take(_ event: NSEvent) -> NSEvent? {
            guard let window, event.window === window,
                event.type == .rightMouseDown || event.modifierFlags.contains(.control)
            else { return event }
            let at = convert(event.locationInWindow, from: nil)
            guard bounds.contains(at), clicked?(at) == true else { return event }
            return nil
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }
    }
}

/// What a right click on the canvas offers (PLAN 3.23): the iPad's long press's menu (PLAN 4.3),
/// as the Mac shows a menu, over where it clicked. It takes no press: the canvas asks for it.
struct CanvasMenu: NSViewRepresentable {
    /// The latest ask: a count, new with each, and where, view points.
    let asked: (count: Int, at: CGPoint)?
    let deck: DeckActions?
    let clip: (Clipping) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> Flipped { Flipped() }

    func updateNSView(_ view: Flipped, context: Context) {
        let held = context.coordinator
        guard let asked, asked.count != held.shown else { return }
        held.shown = asked.count
        let menu = Self.menu(CanvasOffer.offers(deck, clip: clip))
        // Once this update is done: a menu holds the pointer until it closes.
        DispatchQueue.main.async { _ = menu.popUp(positioning: nil, at: asked.at, in: view) }
    }

    final class Coordinator {
        /// The ask last shown.
        var shown = 0
    }

    /// A view whose points run down from its top, as SwiftUI's do; it takes no press.
    final class Flipped: NSView {
        override var isFlipped: Bool { true }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
    }

    /// `offers` as a menu of the Mac's.
    static func menu(_ offers: [CanvasOffer]) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        for offer in offers { menu.addItem(item(offer)) }
        return menu
    }

    private static func item(_ offer: CanvasOffer) -> NSMenuItem {
        switch offer {
        case .action(let title, let symbol, _, let run):
            let action = MenuRun(run)
            let item = NSMenuItem(title: title, action: #selector(MenuRun.chosen), keyEquivalent: "")
            item.target = action
            // The item keeps what it runs: a menu item's target is not kept.
            item.representedObject = action
            item.image = symbol.flatMap { NSImage(systemSymbolName: $0, accessibilityDescription: nil) }
            return item
        case .menu(let title, let symbol, let children):
            let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            item.image = symbol.flatMap { NSImage(systemSymbolName: $0, accessibilityDescription: nil) }
            let submenu = NSMenu(title: title)
            submenu.autoenablesItems = false
            for child in children { submenu.addItem(Self.item(child)) }
            item.submenu = submenu
            return item
        case .divider:
            return .separator()
        }
    }
}

/// What a menu item runs, as the target its action is sent to.
final class MenuRun: NSObject {
    let run: () -> Void

    init(_ run: @escaping () -> Void) {
        self.run = run
    }

    /// The item chosen. Not `perform`: NSObject's own `perform(_:)` makes that selector ambiguous.
    @objc func chosen() { run() }
}
#endif
