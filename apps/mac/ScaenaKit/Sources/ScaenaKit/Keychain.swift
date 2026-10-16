import Foundation
@preconcurrency import Security

/// The user's own keys, one for each provider, in the Keychain (ADR-0006, ADR-0022): under the
/// app's own service, never in a bundle, and sent to their provider alone.
public struct Keychain: Sendable {
    /// The service the keys are kept under.
    public let service: String

    public init(service: String = "com.scaena.assistant") {
        self.service = service
    }

    /// `provider`'s key, if one is kept.
    public func key(for provider: Provider) -> String? { read(provider.rawValue) }

    /// The user's Geoapify key, which maps are fetched with (ADR-0030, PLAN O.1d), if one is kept.
    public var geoapify: String? { read(Self.geoapifyAccount) }

    /// Keep `key` for Geoapify, in place of any kept; none, or an empty one, takes it away.
    public func setGeoapify(_ key: String?) throws {
        try write(
            key, account: Self.geoapifyAccount, label: "Scaena: the Geoapify key",
            about: "The key Scaena fetches maps from Geoapify with")
    }

    private static let geoapifyAccount = "geoapify"

    private func read(_ account: String) -> String? {
        var query = item(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess, let data = found as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    /// Keep `key` for `provider`, in place of any kept; none, or an empty one, takes it away.
    public func set(_ key: String?, for provider: Provider) throws {
        try write(
            key, account: provider.rawValue, label: "Scaena: the \(provider.name) key",
            about: "The key Scaena's assistant asks \(provider.name)'s models with")
    }

    private func write(_ key: String?, account: String, label: String, about: String) throws {
        let gone = SecItemDelete(item(account) as CFDictionary)
        guard gone == errSecSuccess || gone == errSecItemNotFound else { throw refusal(gone) }
        guard let key, !key.isEmpty else { return }
        var added = item(account)
        added[kSecValueData as String] = Data(key.utf8)
        added[kSecAttrLabel as String] = label
        added[kSecAttrDescription as String] = about
        let status = SecItemAdd(added as CFDictionary, nil)
        guard status == errSecSuccess else { throw refusal(status) }
    }

    /// The providers a key is kept for.
    public var providers: [Provider] { Provider.allCases.filter { key(for: $0) != nil } }

    private func item(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    /// Why the Keychain refused, in its own sentence where it has one; never its status's number
    /// (PLAN 3.18), which the system's sentence for a status it has no words for carries.
    private func refusal(_ status: OSStatus) -> ScaenaError {
        guard let said = SecCopyErrorMessageString(status, nil) as String?, !said.contains(where: \.isNumber) else {
            return ScaenaError(message: "the Keychain refused it")
        }
        return ScaenaError(message: "the Keychain refused it: \(said)")
    }
}
