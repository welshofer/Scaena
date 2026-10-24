import ScaenaKit
import SwiftUI
import Testing

/// The menus' keys follow the Mac and Keynote (lift-up USE-5): ⌘G is Find Next and ⇧⌘G Find
/// Previous, ⌥⌘G groups and ⇧⌥⌘G ungroups, ⌘F finds, ⌘B and ⌘I are Bold and Italic, and no two
/// commands share a key.
@Test func theMenusKeysFollowTheMacAndKeynote() {
    #expect(MenuKey.of("g", shift: false, option: false) == .findNext)
    #expect(MenuKey.of("g", shift: true, option: false) == .findPrevious)
    #expect(MenuKey.of("g", shift: false, option: true) == .group)
    #expect(MenuKey.of("g", shift: true, option: true) == .ungroup)
    #expect(MenuKey.of("f", shift: false, option: false) == .find)
    #expect(MenuKey.of("d", shift: false, option: false) == .duplicate)
    #expect(MenuKey.of("d", shift: true, option: false) == nil)
    #expect(MenuKey.group.key.character == "g" && MenuKey.group.modifiers == [.command, .option])
    #expect(MenuKey.ungroup.modifiers == [.command, .option, .shift])
    #expect(MenuKey.findNext.modifiers == .command && MenuKey.findPrevious.modifiers == [.command, .shift])
    #expect(MenuKey.bold.key.character == "b" && MenuKey.italic.key.character == "i" && MenuKey.link.key.character == "k")
    #expect([MenuKey.bold, .italic, .link].allSatisfy { $0.modifiers == .command })
    #expect(MenuKey.bringForward.key.character == "]" && MenuKey.bringToFront.modifiers == [.command, .option])
    let keys = MenuKey.allCases.map { "\($0.key.character) \($0.modifiers.rawValue)" }
    #expect(Set(keys).count == keys.count, "no two commands share a key: \(keys)")
}
