#if !os(macOS)
import ScaenaKit
import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// A pinch on the canvas (PLAN 4.3), as the Mac's trackpad pinch (PLAN 3.16): the slide grows on
/// the desk about where the fingers are, as close as they have spread from the level it began at.
/// The fingers are read on the desk, which stays where it is as the canvas's view grows over it.
/// A second finger touching down stops the first finger's press at once, before the pinch is
/// anything: a long press is one finger's, and what two fingers do edits nothing.
struct CanvasPinch: UIGestureRecognizerRepresentable {
    /// How close the slide is now: 1, fitted to the desk.
    let level: () -> Double
    /// Zoom to a level about a point on the desk, points.
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
            zoom(held.level * Double(pinch.scale), context.converter.location(in: .named(Zoom.space)))
        case .changed:
            zoom(held.level * Double(pinch.scale), context.converter.location(in: .named(Zoom.space)))
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
    /// The fingers moved on the desk, points.
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
    /// Where what it is for stands, view points: the menu stands clear of it, above it where there
    /// is room, as iPadOS puts an edit menu over what it acts on. None where it is for the slide.
    var around: CGRect? = nil

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> Host {
        let view = Host()
        view.backgroundColor = .clear
        view.held = context.coordinator
        let menus = UIEditMenuInteraction(delegate: context.coordinator)
        view.addInteraction(menus)
        context.coordinator.menus = menus
        return view
    }

    func updateUIView(_ view: Host, context: Context) {
        let held = context.coordinator
        held.deck = deck
        held.clip = clip
        held.around = around
        guard let asked, asked.count != held.shown else { return }
        held.shown = asked.count
        view.take()
        held.menus?.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: asked.at))
    }

    /// The view the menu stands on. While the menu shows, it has the keyboard and answers the
    /// system's own Paste, which iPadOS shows only where what has the keyboard can paste; when the
    /// menu goes, the keyboard goes back to what had it.
    final class Host: UIView {
        weak var held: Coordinator?
        /// What had the keyboard before the menu showed.
        private weak var had: UIResponder?

        override var canBecomeFirstResponder: Bool { true }

        override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
            action == #selector(UIResponderStandardEditActions.paste(_:))
        }

        override func paste(_ sender: Any?) { held?.clip(.paste) }

        /// Takes the keyboard for the menu.
        func take() {
            guard !isFirstResponder, let window else { return }
            had = Self.responder(in: window)
            becomeFirstResponder()
        }

        /// Gives the keyboard back to what had it.
        func giveBack() {
            guard isFirstResponder else { return }
            resignFirstResponder()
            if let had, (had as? UIView)?.window != nil { had.becomeFirstResponder() }
            had = nil
        }

        private static func responder(in view: UIView) -> UIView? {
            if view.isFirstResponder { return view }
            for inside in view.subviews {
                if let found = responder(in: inside) { return found }
            }
            return nil
        }
    }

    final class Coordinator: NSObject, UIEditMenuInteractionDelegate {
        var menus: UIEditMenuInteraction?
        var deck: DeckActions?
        var clip: (Clipping) -> Void = { _ in }
        /// Where what the menu is for stands, view points.
        var around: CGRect?
        /// The ask last shown.
        var shown = 0

        func editMenuInteraction(
            _ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration,
            suggestedActions: [UIMenuElement]
        ) -> UIMenu? {
            UIMenu(children: CanvasMenu.items(deck, clip: clip, paste: CanvasMenu.paste(in: suggestedActions)))
        }

        func editMenuInteraction(
            _ interaction: UIEditMenuInteraction, willDismissMenuFor configuration: UIEditMenuConfiguration,
            animator: any UIEditMenuInteractionAnimating
        ) {
            animator.addCompletion { (interaction.view as? Host)?.giveBack() }
        }

        /// The menu stands clear of what it is for, as much of it as the canvas shows; at the press
        /// where it is for the slide.
        func editMenuInteraction(
            _ interaction: UIEditMenuInteraction, targetRectFor configuration: UIEditMenuConfiguration
        ) -> CGRect {
            let pressed = CGRect(origin: configuration.sourcePoint, size: .zero)
            guard let around, let view = interaction.view else { return pressed }
            let shown = around.intersection(view.bounds)
            return shown.isNull || shown.isEmpty ? pressed : shown
        }
    }

    /// What the menu offers (`CanvasOffer`, as the Mac's right click offers it, PLAN 3.23), as
    /// iPadOS's edit menu shows it: no lines between kinds. `paste`, where the system suggests it,
    /// stands in for the menu's own Paste: iPadOS asks to allow a paste an app's own action reads,
    /// and the app's main thread waits on the answer, but asks nothing of its own Paste command,
    /// which the menu's view answers while it shows (`Host.paste`).
    static func items(
        _ deck: DeckActions?, clip: @escaping (Clipping) -> Void, paste: UIMenuElement? = nil
    ) -> [UIMenuElement] {
        CanvasOffer.offers(deck, clip: clip).compactMap { offer in
            if let paste, case .action(CanvasOffer.paste, _, _, _) = offer { return paste }
            return element(offer)
        }
    }

    /// The system's own Paste among what it suggests for the menu: there while what has the
    /// keyboard can paste, the canvas's keys among them.
    static func paste(in suggested: [UIMenuElement]) -> UIMenuElement? {
        for element in suggested {
            if let command = element as? UICommand,
                command.action == #selector(UIResponderStandardEditActions.paste(_:))
            {
                return command
            }
            if let menu = element as? UIMenu, let found = paste(in: menu.children) { return found }
        }
        return nil
    }

    private static func element(_ offer: CanvasOffer) -> UIMenuElement? {
        switch offer {
        case .action(let title, let symbol, let destructive, let run):
            let attributes: UIMenuElement.Attributes = destructive ? .destructive : []
            return UIAction(title: title, image: symbol.flatMap { UIImage(systemName: $0) }, attributes: attributes) { _ in
                run()
            }
        case .menu(let title, let symbol, let children):
            return UIMenu(title: title, image: symbol.flatMap { UIImage(systemName: $0) }, children: children.compactMap(element))
        case .divider:
            return nil
        }
    }
}
#endif
