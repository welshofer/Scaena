// swift-tools-version: 6.0
// ScaenaKit: the session the browser edits, for Swift (PLAN 3.1, ADR-0021), over the C ABI
// `crates/scaena-ffi` makes. Build the library first (`cargo build -p scaena-ffi`) and hand its
// folder to the linker: `swift test -Xlinker -L<repo>/target/debug` and the system libraries
// rustc names (`just ffi`'s list; CI's macOS job passes them).
import PackageDescription

let package = Package(
    name: "ScaenaKit",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "ScaenaKit", targets: ["ScaenaKit"]),
        // The app's executable; `apps/mac/build-app.sh` puts it in `Scaena.app` (PLAN 3.3).
        .executable(name: "Scaena", targets: ["Scaena"]),
    ],
    targets: [
        // `scaena.h`, from where cbindgen writes it, and the static library it declares.
        .systemLibrary(name: "CScaena", path: "Sources/CScaena"),
        .target(name: "ScaenaKit", dependencies: ["CScaena"]),
        // SwiftUI's document protocols are not yet annotated for Swift 6's isolation checking
        // the way this app uses them; the app builds in Swift 5 mode, ScaenaKit in 6.
        .executableTarget(name: "Scaena", dependencies: ["ScaenaKit"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "ScaenaKitTests", dependencies: ["ScaenaKit"]),
    ]
)
