import SwiftUI

/// The keys of the menus' commands the canvas may take itself (PLAN 3.11, 3.16, 4.6), one table
/// for the Mac's menus and the iPad's keys, in macOS's and Keynote's conventions (lift-up USE-5):
/// ⌘G is Find Next and ⇧⌘G Find Previous, as on every Mac; ⌥⌘G groups and ⇧⌥⌘G ungroups, as
/// Keynote's do; Find is under Edit, Bold and Italic under Format. Each is one command of one key,
/// so Help's search finds every command in a menu.
public enum MenuKey: CaseIterable, Sendable {
    case duplicate
    case group
    case ungroup
    case lock
    case copyLook
    case pasteLook
    case find
    case findNext
    case findPrevious
    case bringForward
    case sendBackward
    case bringToFront
    case sendToBack
    case bold
    case italic
    case link

    /// Its key, as the menu shows it: the character the key types with nothing held.
    public var key: KeyEquivalent {
        switch self {
        case .duplicate: "d"
        case .group, .ungroup, .findNext, .findPrevious: "g"
        case .lock: "l"
        case .copyLook: "c"
        case .pasteLook: "v"
        case .find: "f"
        case .bringForward, .bringToFront: "]"
        case .sendBackward, .sendToBack: "["
        case .bold: "b"
        case .italic: "i"
        case .link: "k"
        }
    }

    /// What is held with it: ⌘ always.
    public var modifiers: EventModifiers {
        switch self {
        case .duplicate, .findNext, .find, .bringForward, .sendBackward, .bold, .italic, .link: .command
        case .group, .copyLook, .pasteLook, .bringToFront, .sendToBack: [.command, .option]
        case .ungroup: [.command, .option, .shift]
        case .lock, .findPrevious: [.command, .shift]
        }
    }

    /// The command of `character`'s key pressed with ⌘, and Shift and Option as given, if any:
    /// what a press on the canvas does (PLAN 4.6).
    public static func of(_ character: Character, shift: Bool, option: Bool) -> MenuKey? {
        var held: EventModifiers = .command
        if shift { held.insert(.shift) }
        if option { held.insert(.option) }
        return allCases.first { $0.key.character == character && $0.modifiers == held }
    }
}
