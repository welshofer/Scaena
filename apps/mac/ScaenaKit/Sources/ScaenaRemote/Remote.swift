import CryptoKit
import Foundation
import Network
import Security

/// The presenter remote (PLAN 4.10, SPEC §9.5, ADR-0025): an iPhone or an iPad drives a deck a Mac
/// or an iPad presents, over the local network, and shows its notes and what comes next. The
/// presenter offers the deck by Bonjour; a remote joins it with the code the presenter's editor
/// shows, from which both ends derive TLS's pre-shared key, so a remote without the code cannot
/// connect and no one on the network reads what passes. Each message is JSON, its length before it.
/// This module holds no engine: a remote draws what the presenter sends it.
public enum Remote {
    /// The Bonjour service a presenter offers its deck under.
    public static let service = "_scaena-remote._tcp"
    /// The most a message may hold: a place, its two pictures at a remote's size, and its notes.
    static let largest = 16 << 20

    /// A new code: four digits, as a presentation app's remote pairs with.
    public static func code() -> String {
        String(format: "%04d", Int.random(in: 0..<10_000))
    }

    /// TCP under TLS with a key both ends derive from `code`, the identity it is known by in the
    /// handshake the service's: Apple's pattern for a peer-to-peer protocol.
    public static func parameters(code: String) -> NWParameters {
        let tls = NWProtocolTLS.Options()
        let identity = Data("Scaena Remote".utf8)
        let mac = HMAC<SHA256>.authenticationCode(for: identity, using: SymmetricKey(data: Data(code.utf8)))
        let key = mac.withUnsafeBytes { DispatchData(bytes: $0) }
        let name = identity.withUnsafeBytes { DispatchData(bytes: $0) }
        sec_protocol_options_add_pre_shared_key(tls.securityProtocolOptions, key as __DispatchData, name as __DispatchData)
        sec_protocol_options_append_tls_ciphersuite(
            tls.securityProtocolOptions, tls_ciphersuite_t(rawValue: UInt16(TLS_PSK_WITH_AES_128_GCM_SHA256))!)
        let tcp = NWProtocolTCP.Options()
        tcp.enableKeepalive = true
        tcp.keepaliveIdle = 2
        let parameters = NWParameters(tls: tls, tcp: tcp)
        parameters.includePeerToPeer = true
        return parameters
    }
}

/// Where a show is, as a presenter tells its remotes (PLAN 4.10). A place in the deck is a state
/// and a time into its cue (SPEC §2.4); with the state's notes, and the state and the next drawn at
/// rest, for a remote that holds no deck.
public struct RemotePlace: Codable, Sendable, Equatable {
    /// What the presenter calls the deck.
    public var deck: String
    /// Whether a show plays: with none, the rest is empty and the remote may ask for one.
    public var showing: Bool
    public var state: String
    /// Where the state stands among those the show plays, from 0, and how many there are.
    public var index: Int
    public var count: Int
    /// The time into the state's cue, ms, as the place was sent; at rest, `rest`.
    public var ms: Double
    public var rest: Bool
    public var notes: String
    /// The state at rest, and the one after it, each a PNG; none for a state the show lacks.
    public var slide: Data?
    public var next: Data?
    /// When the show began, for the time since.
    public var began: Date?

    public init(
        deck: String, showing: Bool, state: String = "", index: Int = 0, count: Int = 0, ms: Double = 0,
        rest: Bool = true, notes: String = "", slide: Data? = nil, next: Data? = nil, began: Date? = nil
    ) {
        self.deck = deck
        self.showing = showing
        self.state = state
        self.index = index
        self.count = count
        self.ms = ms
        self.rest = rest
        self.notes = notes
        self.slide = slide
        self.next = next
        self.began = began
    }

    /// No show plays.
    public static func waiting(_ deck: String) -> RemotePlace { RemotePlace(deck: deck, showing: false) }
}

/// What a remote asks of the show (PLAN 4.10), as the player's keys and gestures do (PLAN 2.2):
/// play it from the slide the editor shows, go on, go back, the first and the last state, or end it.
public enum RemoteCommand: String, Codable, Sendable, CaseIterable {
    case play
    case on
    case back
    case first
    case last
    case end
}

/// One message: a place from the presenter, or a command from a remote.
struct RemoteMessage: Codable, Sendable, Equatable {
    var place: RemotePlace?
    var command: RemoteCommand?

    /// Its bytes on the wire: the length of its JSON, four bytes big-endian, then the JSON.
    func framed() throws -> Data {
        let body = try JSONEncoder().encode(self)
        let length = UInt32(body.count)
        var frame = Data([UInt8(length >> 24), UInt8(length >> 16 & 0xff), UInt8(length >> 8 & 0xff), UInt8(length & 0xff)])
        frame.append(body)
        return frame
    }

    /// A frame's length, from its first four bytes.
    static func length(_ header: Data) -> Int {
        header.prefix(4).reduce(0) { $0 << 8 | Int($1) }
    }
}

extension NWConnection {
    /// Send `message`, framed.
    func send(_ message: RemoteMessage) {
        guard let frame = try? message.framed() else { return }
        send(content: frame, completion: .contentProcessed { _ in })
    }

    /// Each message that comes, in turn, on the main queue; nil once none can, and no more after.
    func receiveMessages(_ take: @escaping (RemoteMessage?) -> Void) {
        receive(minimumIncompleteLength: 4, maximumLength: 4) { [weak self] header, _, _, error in
            guard let self, let header, header.count == 4, error == nil else { return take(nil) }
            let length = RemoteMessage.length(header)
            guard length > 0, length <= Remote.largest else { return take(nil) }
            self.receive(minimumIncompleteLength: length, maximumLength: length) { [weak self] body, _, _, error in
                guard let self, let body, body.count == length, error == nil,
                    let message = try? JSONDecoder().decode(RemoteMessage.self, from: body)
                else { return take(nil) }
                take(message)
                self.receiveMessages(take)
            }
        }
    }
}
