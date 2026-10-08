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
    public func key(for provider: Provider) -> String? {
        var query = item(provider)
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
        let gone = SecItemDelete(item(provider) as CFDictionary)
        guard gone == errSecSuccess || gone == errSecItemNotFound else { throw refusal(gone) }
        guard let key, !key.isEmpty else { return }
        var added = item(provider)
        added[kSecValueData as String] = Data(key.utf8)
        added[kSecAttrLabel as String] = "Scaena: the \(provider.name) key"
        added[kSecAttrDescription as String] = "The key Scaena's assistant asks \(provider.name)'s models with"
        let status = SecItemAdd(added as CFDictionary, nil)
        guard status == errSecSuccess else { throw refusal(status) }
    }

    /// The providers a key is kept for.
    public var providers: [Provider] { Provider.allCases.filter { key(for: $0) != nil } }

    private func item(_ provider: Provider) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: provider.rawValue,
        ]
    }

    private func refusal(_ status: OSStatus) -> ScaenaError {
        let said = SecCopyErrorMessageString(status, nil) as String? ?? "status \(status)"
        return ScaenaError(message: "the Keychain refused: \(said)")
    }
}
