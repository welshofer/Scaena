import Foundation

/// What a key the user gives is, before it is kept and each time the settings show it: its form,
/// checked as it is typed, then its provider's own answer to the request that lists the models it
/// can use, the cheapest call each provider answers, and refuses a bad key to. Nothing here keeps a
/// key, and a key goes nowhere but to its provider (ADR-0006, ADR-0022).
public enum KeyCheck {
    /// What a provider said of a key.
    public enum Verdict: Equatable, Sendable {
        /// The key works: with the number of models it can use, where the answer listed them.
        case accepted(models: Int?)
        /// The provider refused the key, in a sentence that says why.
        case refused(String)
        /// The provider could not be asked, or answered with an error of its own, in a sentence.
        case unreachable(String)

        /// Whether the key works.
        public var works: Bool {
            if case .accepted = self { true } else { false }
        }
    }

    /// `key` as the settings show one kept: its first seven characters, which say whose it is, and
    /// its last four, which say which; never the rest. A key too short to show so is dots.
    public static func masked(_ key: String) -> String {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard key.count >= 16 else { return String(repeating: "•", count: max(key.count, 4)) }
        return "\(key.prefix(7))…\(key.suffix(4))"
    }

    /// What looks wrong with `key` as `provider`'s before it is sent, in a sentence; none where it
    /// looks right. A key of another form may still work, as a server that speaks OpenAI's API
    /// gives one, so this warns and refuses nothing.
    public static func hint(_ key: String, for provider: Provider) -> String? {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        if key.isEmpty { return nil }
        if key.contains(where: \.isWhitespace) { return "A key has no spaces or line breaks in it." }
        if provider.isFormOf(key) {
            return key.count < 20 ? "That is shorter than \(provider.name)'s keys are." : nil
        }
        if let other = Provider.allCases.first(where: { $0 != provider && $0.isFormOf(key) }) {
            return "This looks like \(other.withArticle) key."
        }
        return "\(provider.name)'s keys start with \(provider.keyStart)."
    }

    /// What `provider`'s answer to the request that lists its models says of the key it was
    /// asked with.
    public static func verdict(_ provider: Provider, status: Int, body: String) -> Verdict {
        let refused = { (said: String?) in
            Verdict.refused(said.map { "\(provider.name) refused this key: \($0)" } ?? "\(provider.name) refused this key.")
        }
        switch status {
        case 200..<300:
            return .accepted(models: (try? provider.models(status: status, body: body)).map(\.count))
        case 429:
            // The key is good; the provider is busy, or its quota spent.
            return .accepted(models: nil)
        case 401, 403:
            return refused(said(body))
        case 400 where body.localizedCaseInsensitiveContains("key"):
            // Gemini answers a bad key with a 400 that says so.
            return refused(said(body))
        case 500...:
            return .unreachable("\(provider.name) answered with an error of its own (\(status)). Try again in a moment.")
        default:
            return refused(said(body) ?? "it answered \(status).")
        }
    }

    /// Asks `provider` whether `key` works, by the request that lists its models, through `send`.
    ///
    /// For the apps' UI tests, `SCAENA_KEY_CHECK` in the environment answers in the provider's
    /// place: `accept`, `refuse`, or `offline`, and nothing is sent.
    public static func check(
        _ key: String, for provider: Provider, send: Assistant.Send = Assistant.urlSession
    ) async -> Verdict {
        switch ProcessInfo.processInfo.environment["SCAENA_KEY_CHECK"] {
        case "accept": return .accepted(models: nil)
        case "refuse": return .refused("\(provider.name) refused this key.")
        case "offline": return .unreachable("The internet connection appears to be offline.")
        default: break
        }
        do {
            var request = try provider.modelsRequest(key: key.trimmingCharacters(in: .whitespacesAndNewlines))
                .urlRequest()
            request.timeoutInterval = 20
            let (data, status) = try await send(request)
            return verdict(provider, status: status, body: String(decoding: data, as: UTF8.self))
        } catch let error as URLError {
            return .unreachable(error.localizedDescription)
        } catch {
            return .unreachable("\(provider.name) could not be asked: \(error)")
        }
    }

    /// The message a provider's error answer carries, `{"error": {"message": …}}` in each of
    /// Anthropic's, OpenAI's, and Gemini's.
    static func said(_ body: String) -> String? {
        guard let data = body.data(using: .utf8),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let error = json["error"] as? [String: Any],
            let message = (error["message"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
            !message.isEmpty
        else { return nil }
        return message
    }
}

extension Provider {
    /// How its keys start.
    public var keyStart: String {
        switch self {
        case .anthropic: "sk-ant-"
        case .openai: "sk-"
        case .gemini: "AIza"
        }
    }

    /// Whether `key` has the form of its keys.
    func isFormOf(_ key: String) -> Bool {
        switch self {
        case .anthropic: key.hasPrefix("sk-ant-")
        case .openai: key.hasPrefix("sk-") && !key.hasPrefix("sk-ant-")
        case .gemini: key.hasPrefix("AIza")
        }
    }

    /// Its name after an article, as a sentence says it: "an Anthropic".
    public var withArticle: String {
        switch self {
        case .anthropic: "an Anthropic"
        case .openai: "an OpenAI"
        case .gemini: "a Gemini"
        }
    }

    /// Where its keys are made.
    public var keysPage: URL {
        switch self {
        case .anthropic: URL(string: "https://console.anthropic.com/settings/keys")!
        case .openai: URL(string: "https://platform.openai.com/api-keys")!
        case .gemini: URL(string: "https://aistudio.google.com/apikey")!
        }
    }
}
