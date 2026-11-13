#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A menu item's symbol, made once and kept for every menu after (#1309's walk, hang 16): the
/// canvas's menu made each item's symbol afresh each time it was asked for, a right click holding
/// the window 0.8 s in the walk, all of it in AppKit's symbol lookup. A symbol that does not exist
/// is kept as none, so it is not looked for again.
@MainActor
public enum MenuSymbols {
    #if os(macOS)
    public typealias Image = NSImage
    #else
    public typealias Image = UIImage
    #endif

    private static var kept: [String: Image?] = [:]

    /// The symbol `name`, as the system draws it, or none where there is no such symbol.
    public static func image(_ name: String) -> Image? {
        if let image = kept[name] { return image }
        #if os(macOS)
        let image = NSImage(systemSymbolName: name, accessibilityDescription: nil)
        #else
        let image = UIImage(systemName: name)
        #endif
        kept[name] = image
        return image
    }

    /// How many symbols are kept, a missing one among them: for a test.
    static var count: Int { kept.count }
}
