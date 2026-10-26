import Foundation

/// A deck's narrative health (PLAN G.9, SPEC §7.7), as `scaena health --json` gives it: how well it
/// tells its story, as Vzestup measures a talk; its scores, each slide as read, and what to look at.
public struct Health: Decodable, Sendable {
    /// One measure: 0–100, none where it is not scored (coherence read by words alone), and what it
    /// says.
    public struct Measure: Decodable, Equatable, Sendable {
        public let score: Int?
        public let says: String
    }

    /// A slide as health reads it: its first state, the arc's beat it was written for, its
    /// headline, where its arc's curve stands at its place, the intensity and the tone it declares
    /// and that its words read, and how it stands: `ok`,
    /// `bends` (its intensity strays from the curve), or `drifted` (written for another beat).
    public struct Read: Decodable, Equatable, Sendable {
        public let state: String
        /// The beat of the arc it was written for, by its place in `beats` (lift-up ATTR-4).
        public let part: Int?
        public let title: String
        public let curve: Double
        public let declared: Double?
        public let read: Double?
        /// The tone it declares, −1 grave to +1 hopeful, and the tone its words read.
        public let tone: Double?
        public let toneRead: Double?
        public let standing: String
    }

    /// A fix: what it does, in a few words, and the patch that does it.
    public struct Fix: Decodable, Sendable {
        public let says: String
        public let patch: [JSONValue]
    }

    /// Something to look at on one slide: what it says, the question for the assistant, and a fix
    /// where one is safe.
    public struct Finding: Decodable, Identifiable, Sendable {
        public var id: String { "\(state) \(kind) \(says)" }
        /// The slide's first state.
        public let state: String
        /// The slide's number, from 1.
        public let slide: Int
        /// `error`, `warning`, or `info`.
        public let severity: String
        /// `title`, `empty`, `turn`, `arc`, `intensity`, `tone`, `drift`, `layout`, `claim` (a beat of
        /// the settled outline its slides do not carry), `bigIdea` (a close that never lands it),
        /// `story` (a narrative warning lint finds), `dense`, `notes` (none to speak from),
        /// `pacing` (the first slide to cut where the deck runs past its brief), `look` (a change
        /// the model asked once it saw the slide, let go: lint found it worse), or `label` (a
        /// content slide's headline that names its topic where it could state its point).
        public let kind: String
        public let says: String
        public let ask: String
        public let fix: Fix?
    }

    public let score: Int
    /// The arc the deck was written to, where it names one.
    public let arc: String?
    /// The arc's beats, by name, in order; none for an arc the catalog does not name, or where
    /// the answer names none.
    public let beats: [String]?
    public let pacing: Measure
    public let balance: Measure
    public let coherence: Measure
    public let fidelity: Measure
    public let intensity: Measure
    public let tone: Measure
    public let slides: [Read]
    public let findings: [Finding]
    /// The big idea the settled outline names, the one sentence the audience should remember,
    /// where it names one (lift-up 10-24b FUNC-1).
    public let bigIdea: String?
}

extension Health {
    /// How many findings stand open before the rest fold under one disclosure: every error, then
    /// the warnings to see to first, up to this many in all (lift-up S1), as the browser's Story tab.
    public static let opened = 5

    /// Where a warning ranks among the warnings: one no measure speaks to first, as it names one
    /// slide's own trouble; then by its measure's score, lowest first; an unscored measure's last.
    private func rank(_ finding: Finding) -> Int {
        let measure: Measure? =
            switch finding.kind {
            case "tone": tone
            case "intensity": intensity
            case "arc", "drift": fidelity
            case "turn", "claim", "bigIdea": coherence
            case "pacing": pacing
            default: nil
            }
        guard let measure else { return -1 }
        return measure.score ?? 101
    }

    /// The findings as the Story panel leads with them (lift-up S1): what to do first open, every
    /// error a slide's lint finds, then the warnings whose measure scores lowest, `open` in all; and
    /// the rest, the infos among them, folded, in the order the slides stand. Each finding once.
    public func triaged(open: Int = Health.opened) -> (open: [Finding], more: [Finding]) {
        let indexed = Array(findings.enumerated())
        let errors = indexed.filter { $0.element.severity == "error" }
        let warnings = indexed.filter { $0.element.severity == "warning" }
            .sorted { (rank($0.element), $0.offset) < (rank($1.element), $1.offset) }
        let first = errors + warnings.prefix(max(0, open - errors.count))
        let kept = Set(first.map(\.offset))
        return (first.map(\.element), indexed.filter { !kept.contains($0.offset) }.map(\.element))
    }

    /// What the folded findings come to, by kind, the most first: "12 more: 7 tone, 5 arc".
    public static func moreSaid(_ more: [Finding]) -> String {
        var counts: [(kind: String, count: Int)] = []
        for finding in more {
            let kind =
                switch finding.kind {
                case "bigIdea": "big idea"
                case "layout": "broken slide"
                case "look": "change let go"
                case "label": "topic headline"
                default: finding.kind
                }
            if let i = counts.firstIndex(where: { $0.kind == kind }) {
                counts[i].count += 1
            } else {
                counts.append((kind, 1))
            }
        }
        let most = counts.enumerated().sorted { ($0.element.count, -$0.offset) > ($1.element.count, -$1.offset) }
        return "\(more.count) more: " + most.map { "\($0.element.count) \($0.element.kind)" }.joined(separator: ", ")
    }
}

extension ScaenaSession {
    /// The deck's narrative health (PLAN G.9): its scores, each slide as read, and what to look at,
    /// as the browser's Story tab shows it.
    public func health() throws -> Health { try call("health") }
}
