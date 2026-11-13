/// What animates in the app's windows with nothing to show moving: none of the app's own, held by
/// the iPad's UI tests on B1 at rest (lift-up 20261113). An editor left open should cost nothing.
/// Some animations are UIKit's own and run while its Liquid Glass is on screen, which nothing of
/// ours starts or can stop, so they are not counted:
/// - `match-*` (`match-position`, `match-bounds`, `match-mesh`, `match-hidden`, and the corner keys):
///   a `CASDFElementLayer` of a glass group's `UISDFElementView` tracking the shape it draws for, a
///   toolbar's or a segmented control's;
/// - `_UILiquidLensView.punchout.*`: the lens a segmented control's selection is drawn through,
///   tracking the segment chosen.
/// Each is Core Animation's match animation, which the render server evaluates, and no loop on the
/// app's thread (#1286's iPad run, `-reportAnimations`: 35 of the first kind, 2 of the second).
public enum IdleAnimations {
    /// Whether an animation's key is one of UIKit's glass, never the app's.
    public static func systems(_ key: String) -> Bool {
        key.hasPrefix("match-") || key.hasPrefix("_UILiquidLensView.punchout.")
    }

    /// The app's own, of each layer's animations as `(what the layer is, its animation's key)`, said
    /// each as `what, key`.
    public static func ours(_ animations: [(layer: String, key: String)]) -> [String] {
        animations.filter { !systems($0.key) }.map { "\($0.layer), \($0.key)" }
    }

    /// What a test reads: how many, then each, `0` where none.
    public static func said(_ ours: [String]) -> String {
        ours.isEmpty ? "0" : "\(ours.count): " + ours.joined(separator: "; ")
    }

    /// How many `said` counts.
    public static func count(of said: String) -> Int? {
        Int(said.prefix { $0.isNumber })
    }
}
