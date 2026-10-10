# ADR-0023: The iPad client shares the Mac's ScaenaKit, and Xcode builds it

**Status:** proposed · **Date:** 2026-10-09

## Context

Jay started Phase 4, an iPad client, on 2026-10-09. Phase 3 made the Mac's client in two parts:
- **ScaenaKit**, a Swift package over the C ABI that wraps the browser's session (ADR-0021).
- **The Scaena app**, a SwiftPM executable that `apps/mac/build-app.sh` wraps in an app bundle.

Most of ScaenaKit is already platform-neutral. Of its 23 files, four use AppKit:
- the Metal view, an `NSView` on a `CAMetalLayer`;
- the canvas's keys, through `NSTextInputClient`;
- typing, which they feed;
- the clipboard, on `NSPasteboard`.

The rest use Foundation and CoreGraphics alone: the session, the editor, the assistant, and each gesture's arithmetic (`Targets.snap`, `Turn`, `KeyedHandle`, `Typing`'s edits).

The engine compiles for iPadOS as it is. `scaena-ffi` type-checks for `aarch64-apple-ios-sim` without a warning, the Metal painter included: wgpu's Metal backend, on the same kind of `CAMetalLayer`.

Three things differ on the iPad:
- **Installing.** An app on an iPad is signed for a team and installed by Xcode. SwiftPM builds an iOS executable, but no app bundle a device takes.
- **Input.** A finger has no hover and needs a target of 44 pt. Gestures take the place of the right click and of many keys.
- **Windows.** The window is resized by Stage Manager and shares the screen with other apps. iPadOS 26 has a menu bar, which takes an app's commands as the Mac's does.

## Decision

1. **One ScaenaKit, for the Mac and the iPad.** The package adds iPadOS 26.
   - What is platform-neutral stays shared, unchanged.
   - Each AppKit piece gets a UIKit twin, behind `#if os(macOS)` and `#if os(iOS)`:
     - `ScaenaView`: a `UIView` on a `CAMetalLayer`, its `CADisplayLink` asking for 120 Hz on ProMotion;
     - the pasteboard: `UIPasteboard`;
     - a picture as a PNG: `UIImage`;
     - the canvas's text input: `UITextInput`, in place of `NSTextInputClient`.
   - ScaenaKit's tests run on both: `swift test` on the Mac, and `xcodebuild test` on an iPad simulator.
2. **The app's sources are the Mac's.** The iPad's app target compiles the Mac app's SwiftUI (`apps/mac/ScaenaKit/Sources/Scaena`) as it is: the window, the state list, the light table, the inspector and its panels, the cue's bar, and the findings.
   - Each AppKit piece sits behind `#if os(macOS)`, with a UIKit twin beside it, as in ScaenaKit.
   - What only the iPad has lives in `apps/ipad/Sources`.
   - The source pane is a text view on each platform; Jay's call on TextKit in chrome (gate 3, criterion 3) covers both.
3. **The iPad app is an Xcode project that XcodeGen makes from `apps/ipad/project.yml`.** The generated project is not kept in the repository.
   - A build phase builds `scaena-ffi` with cargo for the platform Xcode builds for: `aarch64-apple-ios` or `aarch64-apple-ios-sim`, in release for a Release build.
   - The app links the library by search path, as the Mac's app links its own.
   - `just ipad` makes the project and opens it. A team in `apps/ipad/Team.xcconfig`, kept out of the repository, signs it for a device. *(As built: the decision named an environment variable, `SCAENA_TEAM`, which nothing reads.)*
4. **A touch is the Mac's gesture, never a new edit.**
   - Each touch maps to a Mac gesture:
     - a tap is a click;
     - a drag is a drag;
     - a long press is the right click;
     - a double tap is the double click;
     - two fingers pan and pinch, as the Mac's trackpad does.
   - The Pencil is a pointer with hover. A pointer and a keyboard work as on the Mac.
   - Each gesture makes the patch the Mac's makes, through the same ScaenaKit functions; only the events that come in differ.
   - A handle is drawn as the Mac draws it, and a finger takes it within 22 pt: a 44 pt target.
5. **The window follows iPadOS's:**
   - a `DocumentGroup` over the Mac's `ScaenaDocument`, a deck being a `.scaena` package in Files;
   - a split view whose columns fold to the size the window is given;
   - the Mac's commands in iPadOS's menu bar, and by their keys.

## Alternatives

- **The shared chrome as a library (`ScaenaUI`).** It would make each view, its initializers, and the app's own state types public API, to share code that one app target can simply compile.

- **A SwiftPM executable wrapped by a script,** as the Mac's is. It builds and runs on the simulator. A device needs a provisioning profile, which Xcode makes for a project.
- **An Xcode project kept in the repository.** It would be written without Xcode at hand, and reviewed as `project.pbxproj` diffs.
- **A Swift Playgrounds app package (`AppleProductTypes`).** Plain `swift build` cannot read its manifest.
- **The engine as an XCFramework,** a binary target of the package. That is for shipping a binary to others. Here, Xcode builds the engine from the same checkout it builds the app from.

## Consequences

- **+** The editor's behavior is written a second time only where events come in. A fix in ScaenaKit reaches both Apple clients, and ScaenaKit's tests guard both.
- **+** No binary framework to build or keep in step.
- **−** XcodeGen is one more tool to install (`brew install xcodegen`), on CI's Mac too, and the project is generated rather than kept.
- **−** The iPad's tests run on the simulator, whose GPU is the Mac's. A device's pixels and timing are read on a real iPad, as gate 3's are read on a real Mac.
- **−** The Mac app's sources build twice: by SwiftPM for the Mac, and by Xcode for the iPad. CI's macOS and `ipad` jobs each check a change to them.
