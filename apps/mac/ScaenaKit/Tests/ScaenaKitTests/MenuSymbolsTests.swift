import Foundation
@testable import ScaenaKit
import Testing

/// A menu's symbol is made once and kept (#1309's walk, hang 16): the canvas's menu made each
/// item's symbol afresh each time it was asked for. The second ask hands back the image the first
/// made, and a name the system has no symbol for is kept as none, not looked for again.
@Test @MainActor func aMenuSymbolIsMadeOnceAndKept() {
    let first = MenuSymbols.image("doc.on.doc")
    #expect(first != nil, "a symbol the system has")
    #expect(MenuSymbols.image("doc.on.doc") === first, "asked again: the image made before")

    let missing = "no.such.symbol.\(UUID().uuidString)"
    let before = MenuSymbols.count
    #expect(MenuSymbols.image(missing) == nil)
    #expect(MenuSymbols.count == before + 1, "kept as none")
    #expect(MenuSymbols.image(missing) == nil)
    #expect(MenuSymbols.count == before + 1, "not looked for again")
}
