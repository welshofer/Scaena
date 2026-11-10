import Foundation

/// A version of the bundle's history shown read only, away from the window (lift-up 20261110
/// PERF-3): replayed, compared with the deck now, and drawn slide by slide on a session of a
/// snapshot of its own, on a thread of its own, as an export is made (`Snapshot.exported`), so a
/// long history blocks nothing while it is read.
public final class VersionShown: @unchecked Sendable {
    /// The version's states, in order, and each as its own slide list numbers it.
    public let states: [String]
    public let slides: [String]
    /// What changed from it to the deck now.
    public let compared: Compared
    /// The snapshot's session, which shows the version: used by one call at a time, each on
    /// `OffMain`'s thread.
    private let session: ScaenaSession
    private let lock = NSLock()

    private init(session: ScaenaSession, states: [String], slides: [String], compared: Compared) {
        self.session = session
        self.states = states
        self.slides = slides
        self.compared = compared
    }

    /// Version `version` of the deck `snapshot` was taken of, shown and compared.
    public static func opened(_ version: String, from snapshot: ScaenaSession.Snapshot) async throws -> VersionShown {
        let shown = try await OffMain(label: "ScaenaKit.version").run { () -> VersionShown in
            let session = try snapshot.opened()
            let states = try session.viewVersion(version)
            let slides = try session.versionSlides()
            let compared = try session.compareVersions(from: version)
            return VersionShown(session: session, states: states, slides: slides, compared: compared)
        }
        try Task.checkCancellation()
        return shown
    }

    /// `state` of the version at rest, a PNG `width` pixels wide, as `versionPNG` draws it.
    public func png(_ state: String, width: Int) async throws -> Data {
        let data = try await OffMain(label: "ScaenaKit.version").run { [self] () -> Data in
            try lock.withLock { try session.versionPNG(state, width: width) }
        }
        try Task.checkCancellation()
        return data
    }
}
