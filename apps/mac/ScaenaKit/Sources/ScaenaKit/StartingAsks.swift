import Foundation

/// An ask an empty conversation offers, one press to send (lift-up 20261110 ATTR-2): what the
/// button says, and the question it asks. Built from the deck's own narrative health, so the
/// assistant's first screen offers what Scaena reads best, the story, not a sentence of how to ask.
public struct StartingAsk: Equatable, Identifiable, Sendable {
    public var id: String { label }
    public let label: String
    public let question: String

    public init(label: String, question: String) {
        self.label = label
        self.question = question
    }

    /// The asks for a deck whose health is `health`, where it was read: the talk made stronger,
    /// health read first; its weakest beat tightened, where health names its beats and finds
    /// something in one; its figures compared, where health offers it; and every slide held to its
    /// claim.
    public static func offered(by health: Health?) -> [StartingAsk] {
        var offered = [
            StartingAsk(
                label: "Make this talk stronger",
                question: "Read the deck's narrative health first, then make this talk stronger: "
                    + "begin with what it finds first, a slide at a time, and say what you changed.")
        ]
        if let health, let beat = weakest(health) {
            offered.append(
                StartingAsk(
                    label: "Tighten “\(beat)”",
                    question: "Tighten the slides of the “\(beat)” beat: each should say its claim "
                        + "in fewer words, and lose nothing it needs."))
        }
        // The deck's figures compared by the model, where its slides state three or more: health's
        // own words, which the browser offers too.
        if let compare = health?.compare {
            offered.append(StartingAsk(label: compare.label, question: compare.question))
        }
        offered.append(
            StartingAsk(
                label: "Check every slide says its claim",
                question: "Check that every slide says its beat's claim, and list each one that "
                    + "doesn't, with the words you'd give it."))
        return offered
    }

    /// The beat health finds most in, by name: each finding weighs by its severity (an error 3, a
    /// warning 2, an info 1) to the beat its slide was written for; the first of the heaviest
    /// where beats tie. None where health names no beats or finds nothing in one.
    public static func weakest(_ health: Health) -> String? {
        guard let beats = health.beats, !beats.isEmpty else { return nil }
        var parts: [String: Int] = [:]
        for slide in health.slides {
            if let part = slide.part, parts[slide.state] == nil { parts[slide.state] = part }
        }
        var weight = [Int](repeating: 0, count: beats.count)
        for finding in health.findings {
            guard let part = parts[finding.state], beats.indices.contains(part) else { continue }
            weight[part] += finding.severity == "error" ? 3 : finding.severity == "warning" ? 2 : 1
        }
        guard let most = weight.max(), most > 0, let at = weight.firstIndex(of: most) else { return nil }
        return beats[at]
    }
}
