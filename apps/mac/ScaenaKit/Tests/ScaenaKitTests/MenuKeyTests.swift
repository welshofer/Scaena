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
    #expect(MenuKey.group.key.character == "g" && MenuKey.group.modifiers == [.command, .option])
    #expect(MenuKey.ungroup.modifiers == [.command, .option, .shift])
    #expect(MenuKey.findNext.modifiers == .command && MenuKey.findPrevious.modifiers == [.command, .shift])
    #expect(MenuKey.bold.key.character == "b" && MenuKey.italic.key.character == "i" && MenuKey.link.key.character == "k")
    #expect([MenuKey.bold, .italic, .link].allSatisfy { $0.modifiers == .command })
    #expect(MenuKey.bringForward.key.character == "]" && MenuKey.bringToFront.modifiers == [.command, .option])
    // As Keynote's (lift-up 20261031 USE-2): ⌘L locks, ⌘{ ⌘| ⌘} align, ⇧⌘D duplicates the slide.
    #expect(MenuKey.of("l", shift: false, option: false) == .lock)
    #expect(MenuKey.of("l", shift: false, option: true) == nil, "⌥⌘L is the Light Table's")
    #expect(MenuKey.of("[", shift: true, option: false) == .alignLeft, "⇧⌘[ is ⌘{")
    #expect(MenuKey.of("[", shift: false, option: false) == .sendBackward)
    #expect(MenuKey.of("[", shift: false, option: true) == .sendToBack)
    #expect(MenuKey.alignLeft.key.character == "{" && MenuKey.alignLeft.modifiers == .command)
    #expect(MenuKey.of("\\", shift: true, option: false) == .alignCenter)
    #expect(MenuKey.of("]", shift: true, option: false) == .alignRight)
    #expect(MenuKey.of("d", shift: true, option: false) == .duplicateSlide)
    let keys = MenuKey.allCases.map { "\($0.key.character) \($0.modifiers.rawValue)" }
    #expect(Set(keys).count == keys.count, "no two commands share a key: \(keys)")
}

/// ⌘{, ⌘|, and ⌘} set each text selected's lines as the inspector's Alignment does: one patch of a
/// `choose` of `align/x` for each, which the deck takes (lift-up 20261031 USE-2).
@Test func theFormatMenusAlignmentIsOneChooseForEachText() throws {
    let session = try ScaenaSession(directory: b1)
    let patch = choosing(["title", "subtitle"], "align/x", .string("end"), in: "cover")
    #expect(patch.count == 2)
    #expect(patch[0]["op"]?.string == "choose" && patch[0]["prop"]?.string == "align/x")
    #expect(try session.tool("deck_patch", ["ops": .array(patch)]).edited)
    let field = try session.choices(state: "cover", node: "title").fields.first { $0.prop == "align/x" }
    #expect(field?.value?.string == "end")
}
