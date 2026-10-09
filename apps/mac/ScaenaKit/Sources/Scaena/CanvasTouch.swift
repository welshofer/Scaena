#if !os(macOS)
import ScaenaKit
import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// A pinch on the canvas (PLAN 4.3), as the Mac's trackpad pinch (PLAN 3.16): it zooms about where
/// the fingers are, as close as they have spread from the level it began at. A second finger
/// touching down stops the first finger's press at once, before the pinch is anything: a long press
/// is one finger's, and what two fingers do edits nothing.
struct CanvasPinch: UIGestureRecognizerRepresentable {
    /// How close the canvas is now: 1, the whole canvas.
    let level: () -> Double
    /// Zoom to a level about a point on the view, points.
    let zoom: (Double, CGPoint) -> Void
    /// Two fingers down, or `false`, let go.
    let fingers: (Bool) -> Void

    func makeCoordinator(converter: CoordinateSpaceConverter) -> Fingers { Fingers() }

    func makeUIGestureRecognizer(context: Context) -> Pinching {
        let pinch = Pinching()
        pinch.delegate = context.coordinator
        pinch.down = fingers
        return pinch
    }

    func updateUIGestureRecognizer(_ pinch: Pinching, context: Context) {
        pinch.down = fingers
    }

    func handleUIGestureRecognizerAction(_ pinch: Pinching, context: Context) {
        let held = context.coordinator
        switch pinch.state {
        case .began:
            held.level = level()
            fingers(true)
            zoom(held.level * Double(pinch.scale), context.converter.localLocation)
        case .changed:
            zoom(held.level * Double(pinch.scale), context.converter.localLocation)
        case .ended, .cancelled, .failed:
            fingers(false)
        default:
            break
        }
    }
}

/// UIKit's pinch, telling as a second finger touches down, and as the last lets go, before and
/// whether or not it recognizes a pinch.
final class Pinching: UIPinchGestureRecognizer {
    /// Two fingers down, or `false`, every finger let go.
    var down: ((Bool) -> Void)?
    /// The fingers down now.
    private var touching = 0

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        touching += touches.count
        if touching >= 2 { down?(true) }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesEnded(touches, with: event)
        lift(touches.count)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesCancelled(touches, with: event)
        lift(touches.count)
    }

    /// A pinch ended, failed, or was taken away: it hears no more of these fingers.
    override func reset() {
        super.reset()
        if touching >= 2 { down?(false) }
        touching = 0
    }

    private func lift(_ count: Int) {
        let was = touching
        touching = max(0, touching - count)
        if was >= 2, touching == 0 { down?(false) }
    }
}

/// Two fingers moving on the canvas (PLAN 4.3), as the Mac's wheel (PLAN 3.16): what is zoomed in
/// moves with them; a trackpad's two fingers on an iPad do the same. The first finger's press
/// stops being one as it begins.
struct CanvasPan: UIGestureRecognizerRepresentable {
    /// What is shown moved by a move on the view, points, as the fingers moved.
    let pan: (CGVector) -> Void
    /// Two fingers began, or `false`, let go.
    let fingers: (Bool) -> Void

    func makeCoordinator(converter: CoordinateSpaceConverter) -> Fingers { Fingers() }

    func makeUIGestureRecognizer(context: Context) -> UIPanGestureRecognizer {
        let pan = UIPanGestureRecognizer()
        pan.minimumNumberOfTouches = 2
        pan.maximumNumberOfTouches = 2
        pan.allowedScrollTypesMask = .continuous
        pan.delegate = context.coordinator
        return pan
    }

    func handleUIGestureRecognizerAction(_ recognizer: UIPanGestureRecognizer, context: Context) {
        let held = context.coordinator
        let at = recognizer.translation(in: recognizer.view)
        switch recognizer.state {
        case .began:
            held.moved = at
            fingers(true)
        case .changed:
            pan(CGVector(dx: at.x - held.moved.x, dy: at.y - held.moved.y))
            held.moved = at
        case .ended, .cancelled, .failed:
            fingers(false)
        default:
            break
        }
    }
}

/// A right click on the canvas (PLAN 4.5), a trackpad's or a mouse's secondary button: it offers
/// what a long press offers, over where it clicked, as the browser's right click does (PLAN 2.53).
struct CanvasRightClick: UIGestureRecognizerRepresentable {
    /// Clicked at a point on the view, points.
    let clicked: (CGPoint) -> Void

    func makeUIGestureRecognizer(context: Context) -> UITapGestureRecognizer {
        let click = UITapGestureRecognizer()
        click.buttonMaskRequired = .secondary
        return click
    }

    func handleUIGestureRecognizerAction(_ click: UITapGestureRecognizer, context: Context) {
        guard click.state == .ended else { return }
        clicked(context.converter.localLocation)
    }
}

/// What two fingers keep as they move: the level the pinch began at, and how far they had moved.
final class Fingers: NSObject, UIGestureRecognizerDelegate {
    var level = 1.0
    var moved: CGPoint = .zero

    /// The first finger's press, a pinch, and two fingers moving all go on together: the canvas
    /// stops the press itself as two fingers begin.
    func gestureRecognizer(
        _ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        true
    }
}

/// What a long press on the canvas offers (PLAN 4.3): iPadOS's edit menu over what the finger
/// pressed, with the clipboard's Cut, Copy, and Paste, and what the Node menu does to the node
/// selected (PLAN 3.11): Duplicate, Delete, the order and the group, the look, the lock, and
/// Insert, where the finger pressed. It takes no press: the canvas asks for it.
struct CanvasMenu: UIViewRepresentable {
    /// The latest ask: a count, new with each, and where, view points.
    let asked: (count: Int, at: CGPoint)?
    let deck: DeckActions?
    let clip: (Clipping) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        let menus = UIEditMenuInteraction(delegate: context.coordinator)
        view.addInteraction(menus)
        context.coordinator.menus = menus
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        let held = context.coordinator
        held.deck = deck
        held.clip = clip
        guard let asked, asked.count != held.shown else { return }
        held.shown = asked.count
        held.menus?.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: asked.at))
    }

    final class Coordinator: NSObject, UIEditMenuInteractionDelegate {
        var menus: UIEditMenuInteraction?
        var deck: DeckActions?
        var clip: (Clipping) -> Void = { _ in }
        /// The ask last shown.
        var shown = 0

        func editMenuInteraction(
            _ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration,
            suggestedActions: [UIMenuElement]
        ) -> UIMenu? {
            UIMenu(children: CanvasMenu.items(deck, clip: clip))
        }
    }

    /// What the menu offers: what the clipboard and the Node menu can do now, and nothing else.
    static func items(_ deck: DeckActions?, clip: @escaping (Clipping) -> Void) -> [UIMenuElement] {
        func action(
            _ title: String, _ symbol: String, attributes: UIMenuElement.Attributes = [], _ run: @escaping () -> Void
        ) -> UIAction {
            UIAction(title: title, image: UIImage(systemName: symbol), attributes: attributes) { _ in run() }
        }
        var items: [UIMenuElement] = []
        if deck?.duplicate != nil {
            items.append(action("Cut", "scissors") { clip(.cut) })
            items.append(action("Copy", "doc.on.doc") { clip(.copy) })
        }
        items.append(action("Paste", "doc.on.clipboard") { clip(.paste) })
        guard let deck else { return items }
        if let duplicate = deck.duplicate {
            items.append(action("Duplicate", "plus.square.on.square", duplicate))
        }
        if let delete = deck.delete {
            items.append(
                UIMenu(
                    title: "Delete", image: UIImage(systemName: "trash"),
                    children: [
                        action("Delete from This Slide On", "trash", attributes: .destructive) { delete(false) },
                        action("Delete from All Slides", "trash", attributes: .destructive) { delete(true) },
                    ]))
        }
        var arranging: [UIMenuElement] = []
        if let group = deck.group { arranging.append(action("Group", "square.on.square.dashed", group)) }
        if let ungroup = deck.ungroup { arranging.append(action("Ungroup", "square.on.square", ungroup)) }
        if let order = deck.order {
            arranging += [
                action("Bring Forward", "square.2.layers.3d.top.filled") { order("forward") },
                action("Send Backward", "square.2.layers.3d.bottom.filled") { order("backward") },
                action("Bring to Front", "square.3.layers.3d.top.filled") { order("front") },
                action("Send to Back", "square.3.layers.3d.bottom.filled") { order("back") },
            ]
        }
        if !arranging.isEmpty {
            items.append(UIMenu(title: "Arrange", image: UIImage(systemName: "square.3.layers.3d"), children: arranging))
        }
        if let copyLook = deck.copyLook { items.append(action("Copy Style", "paintbrush", copyLook)) }
        if let pasteLook = deck.pasteLook { items.append(action("Paste Style", "paintbrush.pointed", pasteLook)) }
        if let lock = deck.lock {
            items.append(action(deck.locked ? "Unlock" : "Lock", deck.locked ? "lock.open" : "lock", lock))
        }
        if let placeAnew = deck.placeAnew {
            items.append(action("Lay Out on Its Own in This Size", "rectangle.badge.plus", placeAnew))
        }
        // What the theme and the bundle offer, by kind, landing where the finger pressed.
        var kinds: [String] = []
        for insert in deck.inserts where !kinds.contains(insert.kind) { kinds.append(insert.kind) }
        if !kinds.isEmpty {
            let offered = deck.inserts.enumerated().map { (id: $0.offset, insert: $0.element) }
            items.append(
                UIMenu(
                    title: "Insert", image: UIImage(systemName: "plus"),
                    children: kinds.map { kind in
                        UIMenu(
                            title: NodeCommands.kind(kind),
                            children: offered.filter { $0.insert.kind == kind }.map { o in
                                UIAction(title: Words.insertion(o.insert, among: deck.inserts)) { _ in deck.insert(o.id) }
                            })
                    }))
        }
        return items
    }
}
#endif
