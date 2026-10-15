import ImageIO
import SwiftUI

#if os(macOS)
import AppKit
#else
import GameController
import UIKit
#endif

/// What the keyboard holds while the pointer or a finger acts, and how many presses the one now
/// ending makes: AppKit's on the Mac; on the iPad a hardware keyboard's, as GameController reads
/// it, and one press at a time until the canvas counts taps (PLAN 4.2, 4.3).
@MainActor
enum Held {
    /// Shift: off the grid, or one more selected.
    static var shift: Bool {
        #if os(macOS)
        NSEvent.modifierFlags.contains(.shift)
        #else
        down(.leftShift) || down(.rightShift)
        #endif
    }

    /// Option: kept to the state shown.
    static var option: Bool {
        #if os(macOS)
        NSEvent.modifierFlags.contains(.option)
        #else
        down(.leftAlt) || down(.rightAlt)
        #endif
    }

    /// Whether there are keys to hold at all: always on the Mac; on an iPad, while a keyboard is
    /// joined to it. A hint names a key to hold only where one is.
    static var keyboard: Bool {
        #if os(macOS)
        true
        #else
        GCKeyboard.coalesced != nil
        #endif
    }

    /// Command: one more selected, or one taken out.
    static var command: Bool {
        #if os(macOS)
        NSEvent.modifierFlags.contains(.command)
        #else
        down(.leftGUI) || down(.rightGUI)
        #endif
    }

    /// How many presses the one now ending makes: two for a double click. On the iPad, the canvas
    /// counts its own taps (`Presses`).
    static var clicks: Int {
        #if os(macOS)
        NSApp.currentEvent?.clickCount ?? 1
        #else
        1
        #endif
    }

    #if !os(macOS)
    private static func down(_ key: GCKeyCode) -> Bool {
        GCKeyboard.coalesced?.keyboardInput?.button(forKeyCode: key)?.isPressed ?? false
    }
    #endif
}

/// How many presses a press beginning makes, as a double click or a double tap counts them: AppKit
/// counts the Mac's clicks; on the iPad, a press that begins within the double-tap time of the one
/// before it, and a finger's reach of it, is the next of them (PLAN 4.3). Each is timed when its
/// finger came down, not when the canvas hears of it, so a busy moment between the two taps of a
/// double tap leaves them a double tap.
struct Presses {
    /// The press before: where it began, view points, when its finger came down, and how many it
    /// made.
    private var last: (at: CGPoint, when: Date, count: Int)?

    /// How long after one press the next counts with it, seconds: UIKit's double-tap time.
    static let within: TimeInterval = 0.35
    /// How far from the press before, points, the next counts with it: a finger's reach.
    static let near = 22.0

    /// A press began at `point`, view points, its finger down at `time`: how many it makes, one for
    /// a single click or tap.
    @MainActor mutating func began(at point: CGPoint, time: Date) -> Int {
        #if os(macOS)
        return Held.clicks
        #else
        var count = 1
        if let last {
            let gap = time.timeIntervalSince(last.when)
            if gap >= 0, gap <= Self.within, hypot(point.x - last.at.x, point.y - last.at.y) <= Self.near {
                count = last.count + 1
            }
        }
        last = (point, time, count)
        return count
        #endif
    }
}

extension Image {
    /// A picture the engine drew, PNG bytes, as SwiftUI shows it on the Mac and the iPad alike.
    init?(png data: Data) {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        self.init(decorative: image, scale: 1)
    }
}

extension Color {
    /// The color's sRGB components, red, green, blue, and alpha, each 0 to 1 where in gamut.
    var srgb: (red: Double, green: Double, blue: Double, alpha: Double)? {
        #if os(macOS)
        guard let c = NSColor(self).usingColorSpace(.sRGB) else { return nil }
        return (Double(c.redComponent), Double(c.greenComponent), Double(c.blueComponent), Double(c.alphaComponent))
        #else
        var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        guard UIColor(self).getRed(&r, green: &g, blue: &b, alpha: &a) else { return nil }
        return (Double(r), Double(g), Double(b), Double(a))
        #endif
    }
}

/// What the canvas draws over a slide in, its outlines, handles, and caret: the accent the person
/// chose on the Mac; on the iPad the system's blue, which shows on a dark slide as on a light one.
/// The iPad app's own accent is a deep shade of the Dusk accent, which the launch screen's words
/// read on (apps/ipad/Assets.xcassets).
enum Marks {
    static var color: Color {
        #if os(macOS)
        Color.accentColor
        #else
        Color.blue
        #endif
    }
}

/// The gray a slide sits on, as a presentation app's canvas does (PLAN 3.18): the Mac's under-page
/// gray, the iPad's grouped background.
enum Desk {
    static var color: Color {
        #if os(macOS)
        Color(nsColor: .underPageBackgroundColor)
        #else
        Color(uiColor: .secondarySystemBackground)
        #endif
    }
}

#if !os(macOS)
/// The launch screen's New Deck and New from a Prompt… stay in the window under a deck, out of
/// sight, beside the deck's view; while the deck is open VoiceOver is not given them (PLAN 3.18).
/// What `DocumentGroupLaunchScene` keeps of them takes no modifier the app changes after the
/// screen first shows, so the deck's view finds them beside it as it opens and hides them, and
/// gives them back as it closes.
struct LaunchHidden: UIViewRepresentable {
    /// The launch screen's buttons, by their words, as `ScaenaApp` names them.
    static let titles: Set<String> = ["New Deck", "New from a Prompt…"]

    func makeUIView(context: Context) -> Finder {
        let finder = Finder()
        finder.isUserInteractionEnabled = false
        return finder
    }

    func updateUIView(_ finder: Finder, context: Context) {}

    final class Finder: UIView {
        /// What this deck hid, given back as it closes.
        private let hidden = NSHashTable<UIView>.weakObjects()

        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard window != nil else {
                giveBack()
                return
            }
            hide()
            // The deck's view may come to stand beside the launch screen's buttons only after it is
            // in the window, as the deck opens: it looks again as it is laid out, and a moment after.
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: .seconds(1))
                self?.again()
            }
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            again()
        }

        private func again() {
            if window != nil, hidden.count == 0 { hide() }
        }

        /// Up from the deck's view, the first views beside it that read as the launch screen's
        /// buttons.
        private func hide() {
            var view: UIView = self
            while let parent = view.superview {
                for beside in parent.subviews where beside !== view && Self.reads(beside, depth: 0) {
                    beside.accessibilityElementsHidden = true
                    hidden.add(beside)
                }
                if hidden.count > 0 { return }
                view = parent
            }
        }

        private func giveBack() {
            for view in hidden.allObjects { view.accessibilityElementsHidden = false }
            hidden.removeAllObjects()
        }

        /// Whether `object`, or what it holds a few levels down, is a button that reads as one of the
        /// launch screen's. The screen's own buttons, as it shows them, read as their words.
        private static func reads(_ object: NSObject, depth: Int) -> Bool {
            if object.accessibilityTraits.contains(.button) {
                if let label = object.accessibilityLabel, LaunchHidden.titles.contains(label) { return true }
                if let button = object as? UIButton,
                    let title = button.configuration?.title ?? button.currentTitle,
                    LaunchHidden.titles.contains(title)
                {
                    return true
                }
            }
            guard depth < 4 else { return false }
            if let elements = object.accessibilityElements as? [NSObject],
                elements.contains(where: { reads($0, depth: depth + 1) })
            {
                return true
            }
            guard let view = object as? UIView else { return false }
            return view.subviews.contains(where: { reads($0, depth: depth + 1) })
        }
    }
}
#endif

/// Panes side by side, or one above another: split views the user divides on the Mac; on the
/// iPad, stacked, each pane at its own size (PLAN 4.2). A pane alone is in no split view on the
/// Mac (PLAN 3.27): one under the slides' floating column counts that column's width again in the
/// narrowest the window may be.
struct Panes<Content: View>: View {
    let axis: Axis
    @ViewBuilder let content: Content

    var body: some View {
        #if os(macOS)
        Group(subviews: content) { panes in
            if panes.count < 2 {
                ForEach(panes) { pane in pane }
            } else if axis == .horizontal {
                HSplitView { ForEach(panes) { pane in pane } }
            } else {
                VSplitView { ForEach(panes) { pane in pane } }
            }
        }
        #else
        if axis == .horizontal {
            HStack(spacing: 0) { content }
        } else {
            VStack(spacing: 0) { content }
        }
        #endif
    }
}
