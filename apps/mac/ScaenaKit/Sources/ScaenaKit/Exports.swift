import Foundation

/// What a deck is exported as (PLAN 3.8, 2.122): the PDF `scaena export` writes, or with the
/// presenter's notes under each slide as `--notes` writes it; a state at rest as a PNG this many
/// pixels wide; or every slide once its steps have played as PNGs in one zip, each named by its
/// number after `name`.
public enum Export: Sendable, Equatable {
    case pdf
    case notes
    case png(state: String, width: Int)
    case slides(width: Int, name: String)
}

extension ScaenaSession.Snapshot {
    /// `export`'s bytes, made on a session of its own on the deck as the snapshot was taken, on a
    /// thread of its own with `OffMain.stack` bytes of stack, so the window answers while a long
    /// deck's PDF or slides are made (lift-up PERF-3). They are the session's own: the snapshot's
    /// session answers as the one it was taken from. A task cancelled meanwhile throws, its
    /// bytes dropped.
    public func exported(_ export: Export) async throws -> Data {
        let data = try await OffMain(label: "ScaenaKit.export").run { () -> Data in
            let session = try self.opened()
            switch export {
            case .pdf: return try session.pdf()
            case .notes: return try session.pdf(notes: true)
            case .png(let state, let width): return try session.png(state, width: width)
            case .slides(let width, let name): return try session.slides(width: width, name: name)
            }
        }
        try Task.checkCancellation()
        return data
    }
}
