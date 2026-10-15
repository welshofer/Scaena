import Foundation

/// Why something was not done, in a sentence in a person's words (PLAN 3.18), wherever the window
/// says it: a provider's answer that is no answer, by what it means for the person who asked, and
/// any other error as a sentence. No status, no code, none of a provider's own words, no id the
/// deck writes, and no error type's wrapping reach the window. The browser says a provider's
/// answer the same (`answered` in `web/src/words.ts`).
public enum Why {
    /// What an answer that is no answer means for the person who asked.
    enum Answer: Equatable {
        /// The provider did not take the key.
        case notAccepted
        /// The account is out of credit, or past its limit.
        case noCredit
        /// The account cannot use the models asked for.
        case noAccess
        /// The provider's safety system refused what was asked.
        case unsafe
        /// What was asked is more than the model takes at once.
        case tooLong
        /// The provider is busy: asked again in a moment, it answers.
        case busy
        /// The provider is in trouble of its own.
        case down
        /// No answer came: the provider could not be reached.
        case unreached
        /// An answer came that does not read.
        case unread
        /// The provider turned down what was asked, for a reason of its own.
        case refused

        /// What an answer of `status`, its body `body`, means: status 0 where none came. A key not
        /// taken is said by its status alone; else the body's own words decide where they say more
        /// than the status does, so a quota spent is no credit whatever status it came with.
        init(status: Int, body: String) {
            let said = body.lowercased()
            let names = { (words: [String]) in words.contains { said.contains($0) } }
            if status == 0 {
                self = .unreached
            } else if (200..<300).contains(status) {
                // Answered with nothing in it: withheld by the safety system, as Gemini withholds a
                // picture and Anthropic refuses, or an answer that does not read.
                self = names(["safety\"", "prohibited_content", "blocklist", "\"refusal\""]) ? .unsafe : .unread
            } else if status == 401 {
                self = .notAccepted
            } else if status == 402
                || names(["credit", "billing", "payment", "insufficient_quota", "exceeded your current quota"])
            {
                self = .noCredit
            } else if status == 413
                || names([
                    "too long", "too large", "context length", "context window", "maximum context", "too many tokens",
                    "maximum number of tokens",
                ])
            {
                self = .tooLong
            } else if names(["safety", "moderation", "content_policy", "content policy", "usage policy", "flagged"]) {
                self = .unsafe
            } else {
                switch status {
                case 403:
                    // Refused with no words, or of the key itself: not accepted. Else what the key
                    // may use: a model, a region, a project that has not turned the API on.
                    if !said.contains(where: \.isLetter) {
                        self = .notAccepted
                    } else if names(["permission", "access", "not allowed", "region", "country", "territory", "verified"]) {
                        self = .noAccess
                    } else {
                        self = names(["key"]) ? .notAccepted : .noAccess
                    }
                case 400 where names(["api key", "api_key", "x-api-key"]):
                    // Gemini answers a key it does not take with a 400 that says so.
                    self = .notAccepted
                case 404 where names(["model"]):
                    self = .noAccess
                case 408, 429:
                    self = .busy
                case 500...:
                    self = .down
                default:
                    self = .refused
                }
            }
        }
    }

    /// Why `provider`'s answer of `status`, its body `body`, is no answer, in a person's words:
    /// `status` 0 where no answer came.
    public static func answered(_ provider: Provider, status: Int, body: String) -> String {
        let name = provider.name
        switch Answer(status: status, body: body) {
        case .notAccepted: return "\(name) didn't accept your key. Check it, or replace it."
        case .noCredit: return "Your \(name) account is out of credit, or past its limit. Check its plan and billing."
        case .noAccess: return "Your \(name) account can't use these models. Check its plan, or choose another model."
        case .unsafe: return "\(name)'s safety system turned that down. Ask in other words."
        case .tooLong: return "That is more than \(name)'s model takes at once. Ask about less, or start again."
        case .busy: return "\(name) is busy. Try again in a moment."
        case .down: return "\(name) couldn't answer just now. Try again in a moment."
        case .unreached: return unreached(provider)
        case .unread: return "\(name)'s answer couldn't be read. Try again in a moment."
        case .refused: return "\(name) couldn't answer that. Try again in a moment."
        }
    }

    /// That `provider` could not be reached, in a person's words.
    public static func unreached(_ provider: Provider) -> String {
        "\(provider.name) can't be reached. Check your internet connection."
    }

    /// `error` as a sentence a person reads, `provider`'s where a provider was asked: stopped; no
    /// way to the network; the engine's words as they are, which are written for a person, less
    /// the patch's op they name (`op 2: `) and anything from the first id the deck writes; an
    /// error's own description, or the system's, where it has one.
    public static func said(_ error: any Error, of provider: Provider? = nil) -> String {
        if error is CancellationError { return "Stopped." }
        if let error = error as? URLError {
            if error.code == .cancelled { return "Stopped." }
            if let provider { return unreached(provider) }
            return "The network can't be reached. Check your internet connection."
        }
        let words: String
        if let error = error as? ScaenaError {
            words = unwrapped(error.message)
        } else if let described = (error as? LocalizedError)?.errorDescription {
            words = described
        } else if system(error) {
            words = error.localizedDescription
        } else {
            return "Something went wrong. Try again."
        }
        guard let kept = readable(words) else { return "The deck doesn't allow it." }
        return sentence(kept)
    }

    /// Whether `error` is the system's own, which says itself in the person's language: a file's,
    /// or one that carries its description.
    static func system(_ error: any Error) -> Bool {
        let ns = error as NSError
        return ns.domain == NSCocoaErrorDomain || ns.domain == NSPOSIXErrorDomain
            || ns.userInfo[NSLocalizedDescriptionKey] != nil
    }

    /// `message` without the patch's op it names in front: `op 2: unknown node` is unknown node.
    static func unwrapped(_ message: String) -> String {
        guard message.hasPrefix("op "), let colon = message.range(of: ": ") else { return message }
        let op = message[message.index(message.startIndex, offsetBy: 3)..<colon.lowerBound]
        guard !op.isEmpty, op.allSatisfy(\.isNumber) else { return message }
        return String(message[colon.upperBound...])
    }

    /// `words` up to the clause that names something as the deck writes it, an id or a key, which
    /// the engine sets in backticks and is no person's word: "not re-themed: role `title` has no
    /// size" says not re-themed. None where the first clause names one.
    static func readable(_ words: String) -> String? {
        guard let tick = words.firstIndex(of: "`") else { return words }
        let before = words[..<tick]
        let ends = [": ", ". "].compactMap { before.range(of: $0, options: .backwards)?.lowerBound }
        guard let end = ends.max() else { return nil }
        let kept = words[..<end].trimmingCharacters(in: .whitespacesAndNewlines)
        return kept.isEmpty ? nil : kept
    }

    /// `words` as a sentence: its first letter a capital, and a stop at its end where it has none.
    static func sentence(_ words: String) -> String {
        let words = words.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let first = words.first else { return "Something went wrong. Try again." }
        let begun = first.uppercased() + words.dropFirst()
        guard let last = begun.last, !".!?…".contains(last) else { return begun }
        return begun + "."
    }
}
