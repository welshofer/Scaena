import Foundation
import ScaenaKit
import Testing

/// A writing tool the model on the device declines offers the user's own model where a key is kept,
/// and sends it nothing until the person agrees (lift-up STAB-4); without one the decline is said;
/// and a failure that is no decline, or the user's model failing, is not hidden by asking elsewhere.
@Test func aWritingToolTheDeviceDeclinesAsksTheUsersModelOnlyOnceAgreed() async throws {
    struct Broke: Error {}
    let declines = { @Sendable (reason: Declined.Reason) throws -> String in throw Declined(reason) }
    let sent = Sent()
    let elsewhere = Answering.Elsewhere(model: "gpt-x", provider: "OpenAI") {
        await sent.add()
        return "By the key."
    }

    guard case .answered(let text) = try await Answering.answer({ "Tightened." }, else: elsewhere) else {
        Issue.record("the device answers first")
        return
    }
    #expect(text == "Tightened.")

    for reason in [Declined.Reason.tooLong, .guardrails, .language, .notReady] {
        let outcome = try await Answering.answer({ try declines(reason) }, else: elsewhere)
        guard case .declined(let why, let offered?) = outcome else {
            Issue.record("\(reason): the user's model is offered")
            continue
        }
        #expect(why == Declined(reason))
        #expect(await sent.count == 0, "\(reason): nothing is sent before the person agrees")
        #expect(offered.offered == "Ask gpt-x with your OpenAI key")
        #expect(try await offered.agreed() == "By the key.")
        #expect(await sent.count == 1, "\(reason): agreeing asks it once")
        await sent.reset()
    }
    #expect(Declined(.guardrails).said == "Apple Intelligence declined this")

    guard case .declined(let alone, nil) = try await Answering.answer({ try declines(.tooLong) }, else: nil) else {
        Issue.record("with no key, the decline is said and nothing offered")
        return
    }
    #expect(alone == Declined(.tooLong))
    await #expect(throws: Broke.self, "a failure that is no decline asks nowhere else") {
        _ = try await Answering.answer({ throw Broke() }, else: elsewhere)
    }
    #expect(await sent.count == 0)
    let failing = Answering.Elsewhere(model: "gpt-x", provider: "OpenAI") { throw Broke() }
    let failed = try await Answering.answer({ try declines(.language) }, else: failing)
    guard case .declined(_, let offered?) = failed else {
        Issue.record("the failing model is offered")
        return
    }
    await #expect(throws: Broke.self, "and the user's model failing is said as it failed") {
        _ = try await offered.agreed()
    }
}

/// How many times the user's model was asked.
private actor Sent {
    var count = 0
    func add() { count += 1 }
    func reset() { count = 0 }
}
