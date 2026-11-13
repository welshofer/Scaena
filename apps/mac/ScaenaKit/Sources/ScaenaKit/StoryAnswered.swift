import Foundation

extension Health {
    /// What a finding is about, in a few words after its slide's ("slide 2's plan"), as the browser
    /// says what an edit answered (`story.ts`'s `ABOUT`, editor dogfood 6).
    static let about: [String: String] = [
        "title": "title", "empty": "empty place", "turn": "turn", "arc": "plan", "intensity": "intensity",
        "tone": "tone", "drift": "beat", "layout": "layout", "claim": "claim", "bigIdea": "big idea",
        "story": "story", "dense": "points", "notes": "notes", "pacing": "length", "link": "link",
        "look": "change let go", "source": "source", "label": "headline", "thin": "content",
    ]

    /// Each Ask a Story row offers, one for each of `asking`, its findings that ask the assistant
    /// something: "Ask the Assistant" for one alone, else each by what it is about ("Ask About Its
    /// Tone"), a second of a kind numbered, as the browser's Story row names them (`story.ts`'s
    /// `askLabels`, W1's dogfood 17): each question its own, never one joined of them all.
    public static func askLabels(_ asking: [Finding]) -> [String] {
        if asking.count == 1 { return ["Ask the Assistant"] }
        var seen: [String: Int] = [:]
        return asking.map { f in
            let topic = (Self.about[f.kind] ?? f.kind).capitalized
            let n = (seen[topic] ?? 0) + 1
            seen[topic] = n
            return n == 1 ? "Ask About Its \(topic)" : "Ask About Its \(topic) (\(n))"
        }
    }

    /// A finding told apart from the others of a read: by its kind and its slide, the slide by its
    /// state, which a slide folded, moved, or taken away before it leaves as it was, where its
    /// number moves (W1 dogfood 14, as the browser's `places`).
    func told(_ f: Finding) -> String {
        let i = f.slide - 1
        return "\(f.kind) \(slides.indices.contains(i) ? slides[i].state : String(f.slide))"
    }

    /// The findings of `before` that `after` no longer finds, each told apart as a row is
    /// (`StoryPlaces`: by `told`, or by its kind alone where each read has one), on a
    /// state the deck still has, among `states`: what the edits between the two reads answered
    /// (editor dogfood 6, as the browser's `answered`). A slide taken away answers nothing.
    public static func answered(_ before: Health, _ after: Health, states: Set<String>) -> [Finding] {
        let count = { (findings: [Finding]) in Dictionary(findings.map { ($0.kind, 1) }, uniquingKeysWith: +) }
        let (was, now) = (count(before.findings), count(after.findings))
        var found = Set<String>()
        for f in after.findings {
            found.insert(after.told(f))
            if now[f.kind] == 1 { found.insert(f.kind) }
        }
        return before.findings.filter { f in
            states.contains(f.state) && !found.contains(before.told(f))
                && !(was[f.kind] == 1 && found.contains(f.kind))
        }
    }

    /// What an edit answered, as the window's status and the line under the score say it, in the
    /// browser's words: "Answers 2 of the Story's findings: slide 2's plan and intensity", each
    /// slide's in order, the talk's length its own. None where it answered nothing.
    public static func answeredSaid(_ gone: [Finding]) -> String? {
        guard !gone.isEmpty else { return nil }
        var bySlide: [Int: [String]] = [:]
        var length = false
        for f in gone {
            if f.kind == "pacing" {
                length = true
                continue
            }
            let about = Self.about[f.kind] ?? f.kind
            if !(bySlide[f.slide] ?? []).contains(about) { bySlide[f.slide, default: []].append(about) }
        }
        let and = { (words: [String]) in
            words.count < 3 ? words.joined(separator: " and ") : "\(words.dropLast().joined(separator: ", ")) and \(words[words.count - 1])"
        }
        var said = bySlide.sorted { $0.key < $1.key }.map { "slide \($0.key)'s \(and($0.value))" }
        if length { said.append("the talk's length") }
        return "Answers \(gone.count) of the Story's findings: \(said.joined(separator: "; "))"
    }
}
