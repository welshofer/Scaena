#if DEBUG && !os(macOS)
import ObjectiveC
import UIKit

/// No software keyboard for any field of the app's own where it is launched with
/// `-softwareKeyboard NO`, as the iPad's UI tests' hardware pass launches it
/// (`apps/ipad/app-test.sh`, lift-up 20261110). `CanvasKeys` gives the canvas's text input an input
/// view of nothing so; a field in the inspector, a SwiftUI `TextField` over a `UITextField`, still
/// brought iOS's keyboard up, and each key XCTest typed in it hid the keyboard and iOS brought it
/// back. XCTest's wait for the app to go idle then never ended, each event waiting out its two
/// minutes, until a test ran past its ten (#1240's run: `ThemeTests`, the keyboard up at 00:19:14
/// as the theme's Size was typed in). Every `UITextField` and `UITextView` here answers an input
/// view of nothing too, and each key a hardware keyboard sends still reaches it. Debug builds
/// only: a release build reads no flag.
enum Keyboardless {
    private static let nothing = UIView(frame: .zero)

    /// Each field and text view's input view, nothing, where the app was launched so.
    static func installIfAsked() {
        let defaults = UserDefaults.standard
        guard defaults.object(forKey: "softwareKeyboard") != nil, !defaults.bool(forKey: "softwareKeyboard") else { return }
        let asked = #selector(getter: UIResponder.inputView)
        let none: @convention(block) (AnyObject) -> UIView? = { _ in nothing }
        let answer = imp_implementationWithBlock(none)
        for kind in [UITextField.self, UITextView.self] as [AnyClass] {
            guard let inherited = class_getInstanceMethod(kind, asked) else { continue }
            let types = method_getTypeEncoding(inherited)
            // Its own, added where it inherits UIResponder's, so no other responder changes.
            if !class_addMethod(kind, asked, answer, types) {
                method_setImplementation(inherited, answer)
            }
        }
    }
}
#endif
