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

    /// The key kept under `account`. One kept before in a Mac's file-based keychain moves to the
    /// data-protection keychain the first time it is read, where this process may keep it there;
    /// one kept before readable on another device, or while the device is locked, is narrowed.
    private func read(_ account: String) -> String? {
        if let found = copy(item(account)) {
            if found.accessible != Self.accessible as String {
                let narrowed = [kSecAttrAccessible as String: Self.accessible]
                SecItemUpdate(item(account) as CFDictionary, narrowed as CFDictionary)
            }
            return found.key
        }
        guard let key = copy(legacy(account))?.key else { return nil }
        var moved = item(account)
        moved[kSecValueData as String] = Data(key.utf8)
        moved[kSecAttrAccessible as String] = Self.accessible
        if SecItemAdd(moved as CFDictionary, nil) == errSecSuccess {
            SecItemDelete(legacy(account) as CFDictionary)
        }
        return key
    }

    /// The key `query` finds, and how it may be read.
    private func copy(_ query: [String: Any]) -> (key: String, accessible: String?)? {
        var query = query
        query[kSecReturnData as String] = true
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess,
            let attributes = found as? [String: Any], let data = attributes[kSecValueData as String] as? Data,
            let key = String(data: data, encoding: .utf8)
        else { return nil }
        return (key, attributes[kSecAttrAccessible as String] as? String)
    }

    /// Keep `key` for `provider`, in place of any kept; none, or an empty one, takes it away.
    public func set(_ key: String?, for provider: Provider) throws {
        try write(
            key, account: provider.rawValue, label: "Scaena: the \(provider.name) key",
            about: "The key Scaena's assistant asks \(provider.name)'s models with")
    }

    private func write(_ key: String?, account: String, label: String, about: String) throws {
        for query in [item(account), legacy(account)] {
            let gone = SecItemDelete(query as CFDictionary)
            guard gone == errSecSuccess || gone == errSecItemNotFound || gone == errSecMissingEntitlement else {
                throw refusal(gone)
            }
        }
        guard let key, !key.isEmpty else { return }
        var added = item(account)
        added[kSecValueData as String] = Data(key.utf8)
        added[kSecAttrLabel as String] = label
        added[kSecAttrDescription as String] = about
        added[kSecAttrAccessible as String] = Self.accessible
        var status = SecItemAdd(added as CFDictionary, nil)
        if status == errSecMissingEntitlement {
            // A Mac build signed without a keychain group (ad hoc, or `swift test`) has no
            // data-protection keychain: the key is kept in the file-based one, as before.
            var kept = legacy(account)
            kept[kSecValueData as String] = Data(key.utf8)
            kept[kSecAttrLabel as String] = label
            kept[kSecAttrDescription as String] = about
            status = SecItemAdd(kept as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw refusal(status) }
    }

    /// The providers a key is kept for.
    public var providers: [Provider] { Provider.allCases.filter { key(for: $0) != nil } }

    /// How a key may be read: only while the device is unlocked, and never on another device, so
    /// no backup or sync carries it off.
    private static var accessible: CFString { kSecAttrAccessibleWhenUnlockedThisDeviceOnly }

    /// A key's item in the data-protection keychain, the iPad's and, where the app is signed for
    /// it, the Mac's: readable by this app alone, unlike the Mac's file-based keychain.
    private func item(_ account: String) -> [String: Any] {
        var item = legacy(account)
        item[kSecUseDataProtectionKeychain as String] = true
        return item
    }

    /// A key's item wherever the Keychain looks by default: on a Mac, the file-based keychain,
    /// where a key was kept before.
    private func legacy(_ account: String) -> [String: Any] {
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
