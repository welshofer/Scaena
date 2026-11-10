#if os(iOS)
import UIKit

/// Where the software keyboard stands against the desk (lift-up 20261110): its end frame, which
/// UIKit gives in the screen's coordinates, against the desk's frame in its window, converted
/// through the window's screen, so a window in Stage Manager, offset on the screen, reads it right.
@MainActor
public enum KeyboardCover {
    /// How much of the desk, at `desk` in `window`'s coordinates, the keyboard at `keyboard` on the
    /// screen covers, points; none with no window to convert through.
    public static func covered(by keyboard: CGRect, desk: CGRect, in window: UIWindow?) -> CGFloat {
        guard let window else { return 0 }
        let onScreen = window.convert(desk, to: window.screen.coordinateSpace)
        return Zoom.covered(by: keyboard, desk: onScreen)
    }

    /// The key window of the app's scenes: the one the keyboard types into.
    public static var window: UIWindow? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
    }
}
#endif
