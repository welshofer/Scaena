import CryptoKit
import Foundation
import Network
import ScaenaNotes

/// The presenter remote (PLAN 4.10, SPEC §9.5, ADR-0025): an iPhone or an iPad drives a deck a Mac
/// or an iPad presents, over the local network, and shows its notes and what comes next. The
/// presenter offers the deck by Bonjour under its device's name; a remote joins it with the code the
/// presenter's editor shows, by which both ends pair (`Pairing`, lift-up SEC-5): a key exchange the
/// code authenticates, from which a recorded join tells nothing of the code (`Pairing`), and a
/// wrong code is one try, online, of the few the presenter allows before it makes the code anew. Every message after
/// is sealed under the keys the pairing made (`Sealed`), so no one on the network reads what passes.
/// Each message is JSON, its length before it. This module holds no engine: a remote draws what the
/// presenter sends it.
public enum Remote {
    /// The Bonjour service a presenter offers its deck under.
    public static let service = "_scaena-remote._tcp"
    /// The most a message may hold: a place, its two pictures at a remote's size, and its notes,
    /// which only a remote reads, from a presenter it has paired with.
    static let largest = 16 << 20
    /// The most a presenter reads in a message: a hello is some 90 bytes, a proof some 55, and a
    /// sealed command some 45, so no one in range makes it hold more before pairing.
    static let small = 1 << 10
    /// How many hellos a presenter works out a second: the rest are let go unanswered.
    static let hellos = 10
    /// The pairing's version, which each hello says: ends of other versions do not pair.
    static let version = 3
    /// How many wrong codes a presenter takes before it makes its code anew.
    public static let tries = 5
    /// How many remotes may be pairing at once: more are refused until one is done.
    static let pairing = 16

    /// A new code: four digits, as a presentation app's remote pairs with.
    public static func code() -> String {
        String(format: "%04d", Int.random(in: 0..<10_000))
    }

    /// What a presenter offers its deck as, by Bonjour: `device`'s name alone, within the 63 bytes a
    /// service's name may hold, so the room is not told the deck's file name (lift-up SEC-5).
    public static func serviceName(device: String) -> String {
        var name = device.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty { name = "Scaena" }
        while name.utf8.count > 63 { name.removeLast() }
        return name
    }

    /// TCP with keep-alives, over the local network, or with `loopback` on this machine alone, as a
    /// test listens. Nothing passes in the clear over it: each end pairs by the code first, and
    /// every message after is sealed.
    public static func parameters(loopback: Bool = false) -> NWParameters {
        let tcp = NWProtocolTCP.Options()
        tcp.enableKeepalive = true
        tcp.keepaliveIdle = 2
        let parameters = NWParameters(tls: nil, tcp: tcp)
        parameters.includePeerToPeer = true
        if loopback { parameters.requiredInterfaceType = .loopback }
        return parameters
    }
}

/// How a remote and a presenter pair by the code (lift-up SEC-5): a password-authenticated key
/// exchange, SPEKE on P-256. The code is hashed to a point of the curve (`base`); each end raises it
/// to a secret of its own and sends the result, a point that is random whatever the code, and
/// raises the other's to its secret: the same key at both ends, never sent. So a join recorded on
/// the network tells nothing of the code, however many are tried against it offline: every code's
/// point is on the curve, whose order is prime, so no point a hello carries says anything of which
/// code made it. Each end then proves it holds the key, and so the code, by a tag the other checks:
/// a wrong code is one try, online, which the presenter counts. The channel's keys, one each way,
/// come from the key and both ends' nonces. A point goes as its x alone: a peer's is the point
/// CryptoKit makes of it, either of the two with that x, which raise to the same x.
struct Pairing {
    /// Which end this is: the remote sends its hello first and proves itself first.
    enum Side: String {
        case remote
        case presenter
    }

    /// What each end sends first: the version, its point, and a nonce of its own.
    struct Hello: Codable, Equatable {
        var version: Int
        var key: Data
        var nonce: Data
    }

    /// What each end proves itself by, once it holds the key.
    struct Proof: Codable, Equatable {
        var proof: Data
    }

    /// Why a hello or a proof is refused.
    enum Trouble: Error {
        /// Another version, a key or a nonce of another length, or an x of no point of the curve.
        case hello
        /// A message that was not sealed under the channel's key, or not the next one.
        case sealed
    }

    /// The keys both ends share once paired, and the tags each proves itself by.
    struct Keys {
        let remoteToPresenter: SymmetricKey
        let presenterToRemote: SymmetricKey
        private let proof: SymmetricKey
        /// Both hellos, the remote's first.
        private let transcript: Data

        /// The three keys from `bytes`, read where they lie, never copied into bytes of their own.
        fileprivate init(bytes: UnsafeRawBufferPointer, transcript: Data) {
            remoteToPresenter = SymmetricKey(data: UnsafeRawBufferPointer(rebasing: bytes[0..<32]))
            presenterToRemote = SymmetricKey(data: UnsafeRawBufferPointer(rebasing: bytes[32..<64]))
            proof = SymmetricKey(data: UnsafeRawBufferPointer(rebasing: bytes[64..<96]))
            self.transcript = transcript
        }

        /// What `side` proves itself by: a tag over its name and both hellos, under the proof key.
        func tag(_ side: Side) -> Data {
            Data(HMAC<SHA256>.authenticationCode(for: Data(side.rawValue.utf8) + transcript, using: proof))
        }

        /// Whether `tag` proves `side`, in constant time.
        func proves(_ side: Side, _ tag: Data) -> Bool {
            HMAC<SHA256>.isValidAuthenticationCode(tag, authenticating: Data(side.rawValue.utf8) + transcript, using: proof)
        }
    }

    let side: Side
    /// This end's hello: its point, and its nonce.
    let hello: Hello
    private let secret: P256.KeyAgreement.PrivateKey

    /// Begin pairing on `side` with `code`, its point worked out now: a remote's.
    init(side: Side, code: String) throws {
        try self.init(side: side, base: Self.base(code))
    }

    /// Begin pairing on `side` from the code's point, `base`: a presenter's, worked out when it made
    /// the code, so nothing a joining device can time depends on the code.
    init(side: Side, base: P256.KeyAgreement.PublicKey) throws {
        self.side = side
        secret = P256.KeyAgreement.PrivateKey()
        hello = Hello(version: Remote.version, key: try Self.raised(secret, base), nonce: Self.nonce())
    }

    /// The keys, from the other end's `hello`; `Trouble.hello` where it is of another version, of
    /// another length, or an x of no point of the curve. P-256's order is prime: every point but
    /// the identity, which an x cannot name, generates it all, so there is no cofactor to clear and
    /// no point of low order to refuse.
    func keys(from peer: Hello) throws -> Keys {
        guard peer.version == Remote.version, peer.key.count == 32, peer.nonce.count == 16 else { throw Trouble.hello }
        guard let theirs = try? P256.KeyAgreement.PublicKey(compactRepresentation: peer.key) else { throw Trouble.hello }
        let shared = try secret.sharedSecretFromKeyAgreement(with: theirs)
        guard shared.withUnsafeBytes({ bytes in bytes.contains { $0 != 0 } }) else { throw Trouble.hello }
        let (remote, presenter) = side == .remote ? (hello, peer) : (peer, hello)
        let transcript = remote.key + remote.nonce + presenter.key + presenter.nonce
        let derived = shared.hkdfDerivedSymmetricKey(
            using: SHA256.self, salt: remote.nonce + presenter.nonce,
            sharedInfo: Data("Scaena Remote keys \(Remote.version)".utf8) + transcript, outputByteCount: 96)
        return derived.withUnsafeBytes { Keys(bytes: $0, transcript: transcript) }
    }

    /// What the code's point is hashed from, before the code and a counter.
    static let label = Data("scaena-remote-speke-v1".utf8)

    /// The code hashed to a point of the curve: the base both ends raise. The first x that
    /// SHA-256(`label` ‖ code ‖ counter) gives, from a counter of 0, that is a point's, the point
    /// CryptoKit makes of it; no code of four digits needs more than 16 tries. `Trouble.hello` past
    /// 255, which no code reaches.
    static func base(_ code: String) throws -> P256.KeyAgreement.PublicKey {
        for counter in UInt8.min...UInt8.max {
            let x = Data(SHA256.hash(data: label + Data(code.utf8) + Data([counter])))
            if let point = try? P256.KeyAgreement.PublicKey(compactRepresentation: x) { return point }
        }
        throw Trouble.hello
    }

    /// `point`'s x, 32 bytes: what a hello carries.
    static func x(_ point: P256.KeyAgreement.PublicKey) -> Data {
        Data(point.x963Representation.dropFirst().prefix(32))
    }

    /// `point` raised to `secret`, as its x.
    private static func raised(_ secret: P256.KeyAgreement.PrivateKey, _ point: P256.KeyAgreement.PublicKey) throws
        -> Data
    {
        try secret.sharedSecretFromKeyAgreement(with: point).withUnsafeBytes { Data($0) }
    }

    /// Sixteen random bytes.
    static func nonce() -> Data {
        Data((0..<16).map { _ in UInt8.random(in: .min ... .max) })
    }
}

/// A connection's messages sealed once paired (lift-up SEC-5): each under ChaCha20-Poly1305 with
/// the key for its way, numbered from 0 in its nonce, which the other end counts too, so no message
/// is read on the wire, changed, replayed, or taken out of turn.
final class Sealed {
    private let sending: SymmetricKey
    private let receiving: SymmetricKey
    private var sent: UInt64 = 0
    private var received: UInt64 = 0

    init(keys: Pairing.Keys, side: Pairing.Side) {
        switch side {
        case .remote:
            sending = keys.remoteToPresenter
            receiving = keys.presenterToRemote
        case .presenter:
            sending = keys.presenterToRemote
            receiving = keys.remoteToPresenter
        }
    }

    /// The nonce of message `n`: four zero bytes, then `n` big-endian.
    static func nonce(_ n: UInt64) -> Data {
        var data = Data(repeating: 0, count: 4)
        for shift in stride(from: 56, through: 0, by: -8) {
            data.append(UInt8(truncatingIfNeeded: n >> UInt64(shift)))
        }
        return data
    }

    /// `body` sealed as the next message sent: its nonce, the ciphertext, and the tag.
    func seal(_ body: Data) throws -> Data {
        let nonce = try ChaChaPoly.Nonce(data: Self.nonce(sent))
        let box = try ChaChaPoly.seal(body, using: sending, nonce: nonce)
        sent += 1
        return box.combined
    }

    /// `frame` opened as the next message received; `Pairing.Trouble.sealed` where it is not.
    func open(_ frame: Data) throws -> Data {
        let box = try ChaChaPoly.SealedBox(combined: frame)
        guard Data(box.nonce) == Self.nonce(received) else { throw Pairing.Trouble.sealed }
        let body = try ChaChaPoly.open(box, using: receiving)
        received += 1
        return body
    }
}

/// Where a show is, as a presenter tells its remotes (PLAN 4.10). A place in the deck is a state
/// and a time into its cue (SPEC §2.4); with the state's notes, as written and as the blocks a
/// speaker reads (`read`, SPEC §2.6), and the state and the next drawn at rest, for a remote that
/// holds no deck.
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
    /// The notes as a speaker reads them, lines, a list, or a table; empty from a presenter that
    /// sends none, whose `notes` are then read as lines.
    public var read: [NoteBlock]
    /// The state at rest, and the one after it, each a PNG; none for a state the show lacks.
    public var slide: Data?
    public var next: Data?
    /// When the show began, for the time since.
    public var began: Date?

    public init(
        deck: String, showing: Bool, state: String = "", index: Int = 0, count: Int = 0, ms: Double = 0,
        rest: Bool = true, notes: String = "", read: [NoteBlock] = [], slide: Data? = nil, next: Data? = nil,
        began: Date? = nil
    ) {
        self.deck = deck
        self.showing = showing
        self.state = state
        self.index = index
        self.count = count
        self.ms = ms
        self.rest = rest
        self.notes = notes
        self.read = read
        self.slide = slide
        self.next = next
        self.began = began
    }

    enum CodingKeys: String, CodingKey { case deck, showing, state, index, count, ms, rest, notes, read, slide, next, began }

    /// A place as a presenter sent it, one that sent no `read` read as lines.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        deck = try c.decode(String.self, forKey: .deck)
        showing = try c.decode(Bool.self, forKey: .showing)
        state = try c.decode(String.self, forKey: .state)
        index = try c.decode(Int.self, forKey: .index)
        count = try c.decode(Int.self, forKey: .count)
        ms = try c.decode(Double.self, forKey: .ms)
        rest = try c.decode(Bool.self, forKey: .rest)
        notes = try c.decode(String.self, forKey: .notes)
        read = try c.decodeIfPresent([NoteBlock].self, forKey: .read) ?? []
        slide = try c.decodeIfPresent(Data.self, forKey: .slide)
        next = try c.decodeIfPresent(Data.self, forKey: .next)
        began = try c.decodeIfPresent(Date.self, forKey: .began)
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
}

extension NWConnection {
    /// Send `body` as a frame: the length of it, four bytes big-endian, then it.
    func send(frame body: Data) {
        let length = UInt32(body.count)
        var frame = Data([UInt8(length >> 24), UInt8(length >> 16 & 0xff), UInt8(length >> 8 & 0xff), UInt8(length & 0xff)])
        frame.append(body)
        send(content: frame, completion: .contentProcessed { _ in })
    }

    /// A frame's length, from its first four bytes.
    static func length(_ header: Data) -> Int {
        header.prefix(4).reduce(0) { $0 << 8 | Int($1) }
    }

    /// The next frame's body, on the main queue; nil where none can come or it says it is longer
    /// than `largest`, and the frame is not taken up again after.
    func receiveFrame(largest: Int, _ take: @escaping (Data?) -> Void) {
        receive(minimumIncompleteLength: 4, maximumLength: 4) { [weak self] header, _, _, error in
            guard let self, let header, header.count == 4, error == nil else { return take(nil) }
            let length = Self.length(header)
            guard length > 0, length <= largest else { return take(nil) }
            self.receive(minimumIncompleteLength: length, maximumLength: length) { body, _, _, error in
                guard let body, body.count == length, error == nil else { return take(nil) }
                take(body)
            }
        }
    }
}
