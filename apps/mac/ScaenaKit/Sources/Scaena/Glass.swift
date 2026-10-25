import SwiftUI

extension View {
    /// The surface of chrome that floats over the slide or the desk (lift-up 10-24b ATTR-3): Liquid
    /// Glass in `shape` on macOS and iPadOS 26, as every Apple app's floating controls are there;
    /// before, on the Mac, the material it had, and with `lifted` the shadow that set it off the desk.
    @ViewBuilder func floating(in shape: some Shape, lifted: Bool = false) -> some View {
        if #available(macOS 26, iOS 26, *) {
            glassEffect(.regular, in: shape)
        } else if lifted {
            background(.regularMaterial, in: shape).shadow(color: .black.opacity(0.15), radius: 6, y: 2)
        } else {
            background(.regularMaterial, in: shape)
        }
    }
}
