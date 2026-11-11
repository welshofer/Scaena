import Foundation
import ScaenaKit
import Testing

/// A health read of `slides` slides, `s1` … `sN`, with `findings`, each `(slide, severity, kind)`.
private func answeredRead(slides: Int, _ findings: [(Int, String, String)]) throws -> Health {
    let measure = #"{"score": 90, "says": ""}"#
    let slide = { (n: Int) in
        #"{"state": "s\#(n)", "part": null, "title": "", "curve": 0.5, "declared": null, "read": null, "tone": null, "toneRead": null, "standing": "ok"}"#
    }
    let listed = findings.map { n, severity, kind in
        #"{"state": "s\#(n)", "slide": \#(n), "severity": "\#(severity)", "kind": "\#(kind)", "says": "", "ask": ""}"#
    }
    let json = """
        {"score": 90, "arc": null, "pacing": \(measure), "balance": \(measure), "coherence": \(measure),
         "fidelity": \(measure), "intensity": \(measure), "tone": \(measure),
         "slides": [\((1...slides).map(slide).joined(separator: ","))], "findings": [\(listed.joined(separator: ","))]}
        """
    return try JSONDecoder().decode(Health.self, from: Data(json.utf8))
}

/// What an edit answered of the Story's findings, as the browser says it (editor dogfood 6): the
/// findings a read again no longer finds, told apart as a row is, so pacing moved to another slide
/// answers nothing; on a state the deck still has, so a slide taken away answers nothing; said by
/// slide in order, the talk's length its own; and nothing said where nothing was answered.
@Test func whatAnEditAnsweredIsSaidAsTheBrowserSaysIt() throws {
    let states = Set((1...11).map { "s\($0)" })
    let before = try answeredRead(slides: 11, [(2, "info", "arc"), (2, "warning", "intensity"), (8, "warning", "tone"), (5, "warning", "pacing")])
    // Slide 2's plan set for its place: its two findings go; pacing moves to slide 1.
    let after = try answeredRead(slides: 11, [(8, "warning", "tone"), (1, "warning", "pacing")])
    let gone = Health.answered(before, after, states: states)
    #expect(gone.map { "\($0.slide) \($0.kind)" } == ["2 arc", "2 intensity"])
    #expect(Health.answeredSaid(gone) == "Answers 2 of the Story's findings: slide 2's plan and intensity")
    #expect(Health.answeredSaid([]) == nil, "nothing answered, nothing said")
    // Pacing gone is the talk's length, after the slides'.
    let timed = try answeredRead(slides: 11, [(8, "warning", "tone")])
    #expect(Health.answeredSaid(Health.answered(after, timed, states: states)) == "Answers 1 of the Story's findings: the talk's length")
    // A finding on a slide the deck no longer has answers nothing.
    #expect(Health.answered(before, after, states: states.subtracting(["s2"])).isEmpty)
    // Two of one kind on as many slides are told apart by their slides: one going answers it alone.
    let two = try answeredRead(slides: 11, [(3, "warning", "tone"), (8, "warning", "tone")])
    let one = try answeredRead(slides: 11, [(8, "warning", "tone")])
    #expect(Health.answered(two, one, states: states).map(\.slide) == [3])
}
