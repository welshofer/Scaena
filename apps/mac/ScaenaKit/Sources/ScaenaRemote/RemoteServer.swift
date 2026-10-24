import CryptoKit
import Foundation
import Network

/// The presenter's end of the remote (PLAN 4.10): it listens for remotes, under Bonjour where it is
/// given a name, pairs with each that has its code (`Pairing`), tells each where the show is as it
/// goes, and hands on what each asks, every message sealed. A wrong code is one try: after
/// `Remote.tries` of them the code is made anew, those pairing are let go, and no remote pairs for
/// a while, longer each time, so the code cannot be guessed online either (lift-up SEC-5). Its work
/// is done on the main queue.
@MainActor
public final class RemoteServer {
    /// The code a remote joins with: shown where the audience does not see it, in the editor. Made
    /// anew after `Remote.tries` wrong ones, which `recoded` says.
    public private(set) var code: String
    /// The port it listens on, once it does: until then the listener's port is 0, any port, which
    /// no remote can join.
    public private(set) var port: UInt16?
    /// What a remote asks.
    public var command: (RemoteCommand) -> Void = { _ in }
    /// The slide a remote asks for by its number (lift-up FUNC-5).
    public var slide: (Int) -> Void = { _ in }
    /// How many remotes are joined, as it changes.
    public var joined: (Int) -> Void = { _ in }
    /// Why it stopped listening, where it did.
    public var failed: (String) -> Void = { _ in }
    /// The code made anew, after wrong ones were tried: the sheet shows it.
    public var recoded: (String) -> Void = { _ in }
    /// How many wrong codes have been tried since the code was made, as it changes.
    public var tried: (Int) -> Void = { _ in }

    /// The code's point, worked out as the code is made: what each hello raises (`Pairing.base`).
    private var base: P256.KeyAgreement.PublicKey
    private let listener: NWListener
    /// Each remote by its connection: pairing, then paired, its channel sealed.
    private var remotes: [Int: Joining] = [:]
    /// The last remote's number: each connection is keyed by one of its own, never reused, as an
    /// object's address may be once it is freed while a receive is still on its way.
    private var numbered = 0
    private var place: RemotePlace?
    /// Wrong codes tried since the code was made.
    private var wrong = 0
    /// How often the code was made anew since a remote last joined: each wait before pairing
    /// goes on doubles.
    private var rotations = 0
    /// No remote pairs until then, after the code was made anew.
    private var cooling: ContinuousClock.Instant?
    /// When the hellos of the last second came: no more than `Remote.hellos` are worked out a
    /// second, so no one in range keeps the main actor raising points before any pairs.
    private var hellos: [ContinuousClock.Instant] = []
    /// The first such wait, seconds.
    private let cooldown: Double

    /// Listen with `code`, offered by Bonjour as `name` on the local network; with no name, only to
    /// one who knows the port; with `loopback`, only on this machine, as a test listens. After
    /// `Remote.tries` wrong codes, no remote pairs for `cooldown` seconds, twice that the next time,
    /// and so on up to five minutes.
    public init(name: String?, code: String = Remote.code(), loopback: Bool = false, cooldown: Double = 5) throws {
        self.code = code
        base = try Pairing.base(code)
        self.cooldown = cooldown
        listener = try NWListener(using: Remote.parameters(loopback: loopback))
        if let name { listener.service = NWListener.Service(name: name, type: Remote.service) }
        listener.newConnectionHandler = { [weak self] connection in
            MainActor.assumeIsolated { self?.join(connection) }
        }
        listener.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated {
                guard let self else { return }
                switch state {
                case .ready:
                    self.port = self.listener.port?.rawValue
                case .failed(let error):
                    self.port = nil
                    self.failed("\(error)")
                default:
                    break
                }
            }
        }
        listener.start(queue: .main)
    }

    /// Tell every remote where the show is, and each that joins from now.
    public func tell(_ place: RemotePlace) {
        self.place = place
        for remote in remotes.values where remote.paired {
            tell(place, to: remote)
        }
    }

    /// Stop listening, and let every remote go.
    public func stop() {
        listener.cancel()
        for remote in remotes.values {
            remote.timeout?.cancel()
            remote.connection.cancel()
        }
        remotes = [:]
        joined(0)
    }

    /// One remote's connection: pairing, then paired.
    private final class Joining {
        let connection: NWConnection
        /// The keys once both hellos have passed, before the remote has proved itself.
        var keys: Pairing.Keys?
        /// The channel, once paired.
        var sealed: Sealed?
        /// Given up on a pairing that does not finish.
        var timeout: Task<Void, Never>?
        var paired: Bool { sealed != nil }

        init(_ connection: NWConnection) {
            self.connection = connection
        }
    }

    private func join(_ connection: NWConnection) {
        // No pairing while the code was just made anew, and no more than a few at once.
        if let cooling, ContinuousClock.now < cooling {
            connection.cancel()
            return
        }
        if remotes.values.filter({ !$0.paired }).count >= Remote.pairing {
            connection.cancel()
            return
        }
        numbered += 1
        let id = numbered
        let joining = Joining(connection)
        remotes[id] = joining
        connection.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated {
                guard let self else { return }
                switch state {
                case .ready:
                    self.hear(id)
                case .failed, .cancelled:
                    self.remotes[id]?.timeout?.cancel()
                    self.remotes[id] = nil
                    self.joined(self.count)
                default:
                    break
                }
            }
        }
        // A pairing that is not done in ten seconds is let go: one that sends no proof is no try.
        joining.timeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(10))
            guard !Task.isCancelled, let self, let joining = self.remotes[id], !joining.paired else { return }
            joining.connection.cancel()
        }
        connection.start(queue: .main)
    }

    /// The remote's hello: answer it with this end's, and hear its proof.
    private func hear(_ id: Int) {
        guard let joining = remotes[id] else { return }
        joining.connection.receiveFrame(largest: Remote.small) { [weak self] body in
            MainActor.assumeIsolated { self?.heard(id, hello: body) }
        }
    }

    private func heard(_ id: Int, hello body: Data?) {
        guard let joining = remotes[id] else { return }
        // Past the hellos a second it works out, one is let go unanswered, and is no try.
        let now = ContinuousClock.now
        hellos.removeAll { now - $0 > .seconds(1) }
        guard hellos.count < Remote.hellos else {
            joining.connection.cancel()
            return
        }
        hellos.append(now)
        // No hello of this version is no try at the code.
        guard let body, let hello = try? JSONDecoder().decode(Pairing.Hello.self, from: body),
            let pairing = try? Pairing(side: .presenter, base: base), let keys = try? pairing.keys(from: hello),
            let answer = try? JSONEncoder().encode(pairing.hello)
        else {
            joining.connection.cancel()
            return
        }
        joining.keys = keys
        joining.connection.send(frame: answer)
        joining.connection.receiveFrame(largest: Remote.small) { [weak self] body in
            MainActor.assumeIsolated { self?.heard(id, proof: body) }
        }
    }

    /// A pairing whose hellos have passed with `keys`, as `heard(_:hello:)` leaves one: what a test
    /// hands proofs to in the order it chooses, which the loopback cannot pin.
    func pairing(_ connection: NWConnection, keys: Pairing.Keys) -> Int {
        numbered += 1
        let id = numbered
        let joining = Joining(connection)
        joining.keys = keys
        remotes[id] = joining
        return id
    }

    /// The remotes paired, joined or not yet.
    var paired: Int { remotes.values.filter(\.paired).count }

    func heard(_ id: Int, proof body: Data?) {
        guard let joining = remotes[id], let keys = joining.keys else { return }
        guard let body, let proof = try? JSONDecoder().decode(Pairing.Proof.self, from: body) else {
            joining.connection.cancel()
            return
        }
        guard keys.proves(.remote, proof.proof) else {
            wrongly(joining)
            return
        }
        // Paired: prove this end, seal the channel, say where the show is, and hear what it asks.
        guard let own = try? JSONEncoder().encode(Pairing.Proof(proof: keys.tag(.presenter))) else {
            joining.connection.cancel()
            return
        }
        joining.connection.send(frame: own)
        joining.sealed = Sealed(keys: keys, side: .presenter)
        joining.timeout?.cancel()
        joining.keys = nil
        // A join leaves the wrong codes counted: they are counted since the code was made, so a
        // join, or a connection made to join and dropped, buys no more tries at it.
        rotations = 0
        cooling = nil
        joined(count)
        if let place { tell(place, to: joining) }
        hearCommands(id)
    }

    /// A wrong code: the remote is let go, and after `Remote.tries` the code is made anew, those
    /// pairing let go, and no remote pairs for a while, longer each time. Each let go is dropped
    /// with its keys at once, not when its connection says it is cancelled: a proof already on its
    /// way, its keys made under the old code, then finds nothing, is no try, and never pairs.
    private func wrongly(_ joining: Joining) {
        joining.keys = nil
        joining.connection.cancel()
        wrong += 1
        if wrong >= Remote.tries {
            wrong = 0
            rotations += 1
            cooling = .now + .seconds(min(cooldown * pow(2, Double(rotations - 1)), 300))
            for (id, remote) in remotes where !remote.paired {
                remote.keys = nil
                remote.timeout?.cancel()
                remotes[id] = nil
                remote.connection.cancel()
            }
            // The new code's point, worked out now rather than as a remote joins: every code has one.
            var next = Remote.code()
            var made = try? Pairing.base(next)
            while made == nil {
                next = Remote.code()
                made = try? Pairing.base(next)
            }
            code = next
            if let made { base = made }
            recoded(code)
        }
        tried(wrong)
    }

    /// Each command the remote sends, sealed, in turn; let go at the first that is not.
    private func hearCommands(_ id: Int) {
        guard let joining = remotes[id] else { return }
        joining.connection.receiveFrame(largest: Remote.small) { [weak self] frame in
            MainActor.assumeIsolated {
                guard let self, let joining = self.remotes[id] else { return }
                guard let frame, let sealed = joining.sealed, let body = try? sealed.open(frame),
                    let message = try? JSONDecoder().decode(RemoteMessage.self, from: body)
                else {
                    joining.connection.cancel()
                    return
                }
                if let command = message.command { self.command(command) }
                if let number = message.slide, number > 0 { self.slide(number) }
                self.hearCommands(id)
            }
        }
    }

    private func tell(_ place: RemotePlace, to remote: Joining) {
        guard let sealed = remote.sealed, let body = try? JSONEncoder().encode(RemoteMessage(place: place)),
            let frame = try? sealed.seal(body)
        else { return }
        remote.connection.send(frame: frame)
    }

    /// The remotes joined.
    private var count: Int { remotes.values.filter { $0.paired && $0.connection.state == .ready }.count }
}
