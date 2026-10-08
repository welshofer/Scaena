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
        .library(name: "ScaenaKit", targets: ["ScaenaKit"])
    ],
    targets: [
        // `scaena.h`, from where cbindgen writes it, and the static library it declares.
        .systemLibrary(name: "CScaena", path: "Sources/CScaena"),
        .target(name: "ScaenaKit", dependencies: ["CScaena"]),
        .testTarget(name: "ScaenaKitTests", dependencies: ["ScaenaKit"]),
    ]
)
