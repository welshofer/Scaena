import Foundation

/// What a key the user gives is, before it is kept and each time the settings show it: its form,
/// checked as it is typed, then its provider's own answer to the request that lists the models it
/// can use, the cheapest call each provider answers, and refuses a bad key to. The same request
/// lists a kept key's models for the assistant and New from a Prompt. Nothing here keeps a key,
/// and a key goes nowhere but to its provider (ADR-0006, ADR-0022).
public enum KeyCheck {
    /// What a provider said of a key.
    public enum Verdict: Equatable, Sendable {
        /// The key works: with the number of models it can use, where the answer listed them.
        case accepted(models: Int?)
        /// The provider refused the key, or the account it is of cannot use the models, in a
        /// sentence that says why.
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
    /// asked with, said as the window says it (PLAN 3.18, `Why.answered`): the key not accepted,
    /// or its account out of credit or without the models, is refused; the provider busy or in
    /// trouble, or an answer that says nothing of the key, is not checked. No status, and none of
    /// the provider's own words.
    public static func verdict(_ provider: Provider, status: Int, body: String) -> Verdict {
        if (200..<300).contains(status) {
            return .accepted(models: (try? provider.models(status: status, body: body)).map(\.count))
        }
        let why = Why.answered(provider, status: status, body: body)
        switch Why.Answer(status: status, body: body) {
        case .busy where status == 429:
            // The key is good: the provider is busy.
            return .accepted(models: nil)
        case .notAccepted, .noCredit, .noAccess:
            return .refused(why)
        default:
            return .unreachable(why)
        }
    }

    /// Asks `provider` whether `key` works, by the request that lists its models, through `send`.
    ///
    /// For the apps' UI tests, `SCAENA_KEY_CHECK` in `environment` answers in the provider's
    /// place: `accept`, `refuse`, or `offline`, and nothing is sent.
    public static func check(
        _ key: String, for provider: Provider, environment: [String: String] = ProcessInfo.processInfo.environment,
        send: Assistant.Send = Assistant.urlSession
    ) async -> Verdict {
        switch environment["SCAENA_KEY_CHECK"] {
        case "accept": return .accepted(models: nil)
        case "refuse": return .refused(Why.answered(provider, status: 401, body: ""))
        case "offline": return .unreachable(Why.unreached(provider))
        default: break
        }
        do {
            var request = try provider.modelsRequest(key: key.trimmingCharacters(in: .whitespacesAndNewlines))
                .urlRequest()
            request.timeoutInterval = 20
            let (data, status) = try await send(request)
            return verdict(provider, status: status, body: String(decoding: data, as: UTF8.self))
        } catch {
            // No way to the provider, said in a person's words: not the network's own.
            return .unreachable(Why.said(error, of: provider))
        }
    }

    /// What came of asking a provider for the models a key can use.
    public enum Listing: Equatable, Sendable {
        /// The models, as the provider listed them.
        case listed([String])
        /// None, and why, in a sentence in a person's words.
        case unlisted(String)
    }

    /// The models `key` can use, as `provider` lists them through `send`; where it lists none, why,
    /// as the window says it (PLAN 3.18): the key not accepted, the provider busy or in trouble, or
    /// no way to it, with no status and none of the provider's own words.
    ///
    /// For the apps' UI tests, `SCAENA_KEY_CHECK` in `environment` answers in the provider's place,
    /// as it answers `check`: `accept` lists no models, `refuse` refuses the key, and `offline`
    /// finds no connection; nothing is sent.
    public static func models(
        _ key: String, for provider: Provider, environment: [String: String] = ProcessInfo.processInfo.environment,
        send: Assistant.Send = Assistant.urlSession
    ) async -> Listing {
        switch environment["SCAENA_KEY_CHECK"] {
        case "accept": return .listed([])
        case "refuse": return .unlisted(whyUnlisted(provider, status: 401, body: ""))
        case "offline": return .unlisted(Why.unreached(provider))
        default: break
        }
        do {
            var request = try provider.modelsRequest(key: key.trimmingCharacters(in: .whitespacesAndNewlines))
                .urlRequest()
            request.timeoutInterval = 20
            let (data, status) = try await send(request)
            let body = String(decoding: data, as: UTF8.self)
            if (200..<300).contains(status), let names = try? provider.models(status: status, body: body) {
                return .listed(names)
            }
            return .unlisted(whyUnlisted(provider, status: status, body: body))
        } catch is URLError {
            return .unlisted(Why.unreached(provider))
        } catch {
            return .unlisted(noneListed(provider))
        }
    }

    /// Why `provider`'s answer of `status` lists no models, in a person's words (`Why.answered`):
    /// the key not accepted, the account out of credit or without the models, the provider busy
    /// or in trouble; or an answer that lists none.
    static func whyUnlisted(_ provider: Provider, status: Int, body: String) -> String {
        (200..<300).contains(status) ? noneListed(provider) : Why.answered(provider, status: status, body: body)
    }

    /// That `provider` answered with no models a person can choose, in a person's words.
    static func noneListed(_ provider: Provider) -> String {
        "\(provider.name)'s models couldn't be listed. Try again in a moment."
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
