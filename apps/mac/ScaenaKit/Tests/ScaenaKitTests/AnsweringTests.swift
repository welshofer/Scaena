import Foundation
import ScaenaKit
import Testing

/// A writing tool the model on the device declines is answered by the user's own model where a key
/// is kept, saying why (lift-up STAB-4); without one the decline is said; and a failure that is no
/// decline, or the user's model failing, is not hidden by asking elsewhere.
@Test func aWritingToolTheDeviceDeclinesIsAnsweredByTheUsersModel() async throws {
    struct Broke: Error {}
    let declines = { @Sendable (reason: Declined.Reason) throws -> String in throw Declined(reason) }

    let onDevice = try await Answering.answer({ "Tightened." }, else: { "Never asked." })
    #expect(onDevice.text == "Tightened." && onDevice.declined == nil, "the device answers first")

    for reason in [Declined.Reason.tooLong, .guardrails, .language, .notReady] {
        let asked = try await Answering.answer({ try declines(reason) }, else: { "By the key." })
        #expect(asked.text == "By the key." && asked.declined == Declined(reason), "\(reason): the user's model answers")
    }
    #expect(Declined(.guardrails).said == "Apple Intelligence declined this")

    await #expect(throws: Declined(.tooLong), "with no key, the decline is said") {
        _ = try await Answering.answer({ try declines(.tooLong) }, else: nil)
    }
    await #expect(throws: Broke.self, "a failure that is no decline asks nowhere else") {
        _ = try await Answering.answer({ throw Broke() }, else: { "Never asked." })
    }
    await #expect(throws: Broke.self, "and the user's model failing is said as it failed") {
        _ = try await Answering.answer({ try declines(.language) }, else: { throw Broke() })
    }
}
