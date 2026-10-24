import SwiftUI

/// What a touch holds in a hardware keyboard's place (lift-up FUNC-4): Select, from the iPad's
/// toolbar, which makes each tap add an object to what is selected or take it out, as Shift and a
/// click do on the canvas and ⌘ and a click do in the light table; and Only on This Slide, from a
/// long press's menu, which keeps each gesture to the slide shown, as Option does. Without a
/// keyboard an iPad could select nothing beside what it had, pick no second slide, and keep no edit
/// to one slide. Each is held until it is turned off, and the keys work beside them.
@MainActor @Observable
final class TouchHeld {
    /// Select: a tap toggles what it pressed in the selection.
    var selecting = false
    /// Only on This Slide: a gesture is kept to the slide shown.
    var onlyThisSlide = false

    /// Whether a press adds to the selection or takes from it: Shift, or Select held.
    var extends: Bool { Held.shift || selecting }
    /// Whether a gesture is kept to the slide shown: Option, or Only on This Slide held.
    var forks: Bool { Held.option || onlyThisSlide }
}
