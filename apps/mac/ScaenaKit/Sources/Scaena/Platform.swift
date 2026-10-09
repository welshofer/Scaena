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

    /// Command: one more selected, or one taken out.
    static var command: Bool {
        #if os(macOS)
        NSEvent.modifierFlags.contains(.command)
        #else
        down(.leftGUI) || down(.rightGUI)
        #endif
    }

    /// How many presses the one now ending makes: two for a double click.
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

/// Panes side by side, or one above another: split views the user divides on the Mac; on the
/// iPad, stacked, each pane at its own size (PLAN 4.2).
struct Panes<Content: View>: View {
    let axis: Axis
    @ViewBuilder let content: Content

    var body: some View {
        #if os(macOS)
        if axis == .horizontal {
            HSplitView { content }
        } else {
            VSplitView { content }
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
