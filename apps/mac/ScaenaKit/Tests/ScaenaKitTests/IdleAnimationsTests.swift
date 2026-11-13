import ScaenaKit
import Testing

/// UIKit's glass animations, which run while the glass is on screen, are not the app's; anything
/// else animating at rest is, and is counted (lift-up 20261113).
@Test func theGlassesMatchAndPunchoutAreUIKitsAndEverythingElseIsOurs() {
    // As #1286's iPad run reported them at rest, with two of ours beside them.
    let animating: [(layer: String, key: String)] = [
        ("CASDFElementLayer of UISDFElementView", "match-position"),
        ("CASDFElementLayer of UISDFElementView", "match-mesh"),
        ("CASDFElementLayer of UISDFElementView", "match-corner-radii"),
        ("CALayer of DestOutView", "_UILiquidLensView.punchout.matchPosition"),
        ("CALayer of _SwiftUILayerDelegate", "opacity"),
        ("CAMetalLayer of ScaenaView", "bounds.size"),
    ]
    let ours = IdleAnimations.ours(animating)
    #expect(ours == ["CALayer of _SwiftUILayerDelegate, opacity", "CAMetalLayer of ScaenaView, bounds.size"])
    let said = IdleAnimations.said(ours)
    #expect(IdleAnimations.count(of: said) == 2)
    #expect(said.hasSuffix("CAMetalLayer of ScaenaView, bounds.size"))
    // A key that only mentions a match is not one.
    #expect(!IdleAnimations.systems("position.matchPosition"))
    #expect(!IdleAnimations.systems("transform"))
}

@Test func noneOfOursSaysZero() {
    let said = IdleAnimations.said(IdleAnimations.ours([("CASDFElementLayer of UISDFElementView", "match-bounds")]))
    #expect(said == "0")
    #expect(IdleAnimations.count(of: said) == 0)
}
