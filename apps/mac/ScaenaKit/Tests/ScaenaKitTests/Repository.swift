import Foundation

/// The repository's root, from this file's place in it.
let repository = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()  // ScaenaKitTests
    .deletingLastPathComponent()  // Tests
    .deletingLastPathComponent()  // ScaenaKit
    .deletingLastPathComponent()  // mac
    .deletingLastPathComponent()  // apps
    .deletingLastPathComponent()

/// The repository's file or folder at `path`, or the copy the test bundle carries of it. Gate 4's
/// bundle carries what its tests open, since an iPad reaches none of the Mac's files
/// (`ScaenaGateTests` in `apps/ipad/project.yml`, `docs/gate-4.md`), and reads it on the simulator
/// too, as on an iPad. Every other test reads the repository's.
private func carried(_ path: String) -> URL {
    let name = path.split(separator: "/").last.map(String.init) ?? path
    return Bundle(for: Carried.self).url(forResource: name, withExtension: nil) ?? repository.appending(path: path)
}

/// What finds the test bundle.
private final class Carried {}

/// B1 (SPEC §15): forty states over four families of type.
let b1 = carried("tests/bench/b1.scaena")

/// The torture deck (PLAN 0.2), and the digests of the display lists its goldens hold.
let tortureDeck = carried("tests/fixtures/torture.scaena")
let goldenDigests = carried("tests/golden/torture/raw.fnv1a")

/// The gate a test's reading is for: gate 3's on the Mac, gate 4's on the iPad (PLAN's exit
/// criteria, `docs/gate-3.md`, `docs/gate-4.md`).
#if os(macOS)
let gate = 3
#else
let gate = 4
#endif
