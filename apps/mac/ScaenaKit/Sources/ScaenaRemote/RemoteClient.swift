import Foundation
import Network
import Observation

/// A presenter offering its deck on the local network (PLAN 4.10), as a remote finds it.
public struct RemotePresenter: Identifiable, Hashable, Sendable {
    /// What the presenter calls itself: its device's name.
    public let name: String
    public let endpoint: NWEndpoint

    public var id: String { name }
}

/// The presenters on the local network (PLAN 4.10), by Bonjour, as they come and go.
@MainActor @Observable
public final class RemoteBrowser {
    public private(set) var presenters: [RemotePresenter] = []
    /// Why none can be found, where it is so: the local network refused, say, in a person's words
    /// (PLAN 3.18), not the network's own.
    public private(set) var trouble: String?
    @ObservationIgnored private var browser: NWBrowser?

    public init() {}

    /// Look, until `stop`.
    public func start() {
        guard browser == nil else { return }
        let parameters = NWParameters()
        parameters.includePeerToPeer = true
        let browser = NWBrowser(for: .bonjour(type: Remote.service, domain: nil), using: parameters)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            MainActor.assumeIsolated {
                self?.presenters = results.compactMap { result in
                    guard case .service(let name, _, _, _) = result.endpoint else { return nil }
                    return RemotePresenter(name: name, endpoint: result.endpoint)
                }
                .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            }
        }
        browser.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated {
                switch state {
                case .failed, .waiting: self?.trouble = Self.unfound
                case .ready: self?.trouble = nil
                default: break
                }
            }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    public func stop() {
        browser?.cancel()
        browser = nil
        presenters = []
    }

    /// Why no presenter can be found, and what to do.
    private static let unfound =
        "No presenter can be found. Check that this device is on the presenter's network, and that Local Network is on for it in Settings."
}

/// A remote joined to a presenter (PLAN 4.10): where the show is, as the presenter tells it, and
/// what this remote asks of it. It pairs by the code first (`Pairing`, lift-up SEC-5), and every
/// message after is sealed. The connection, the pairing, and the channel are worked on a queue of
/// their own (`RemoteLink`), so a phone or a test whose main actor is busy still joins within its
/// patience; what the link hears is said here, on the main actor, in turn.
@MainActor @Observable
public final class RemoteClient {
    public enum Status: Equatable, Sendable {
        /// Joining: the pairing, which a wrong code fails.
        case joining
        case joined
        /// It could not join, or the presenter went: why.
        case lost(String)
    }

    public private(set) var status = Status.joining
    /// Where the show is; none until the presenter says.
    public private(set) var place: RemotePlace?
    /// Whether the link waits for the network to come back while joined, as Wi-Fi dropped for a
    /// moment leaves it: the show is shown as it last was, "Reconnecting…" over it (lift-up
    /// 20261110 REL-4).
    public private(set) var reconnecting = false
    /// Whether it was lost after it had joined, the code taken: joining again with the same code
    /// may answer, where a code refused would only spend one of the presenter's tries.
    public private(set) var rejoinable = false
    /// Whether the presenter refused its code: joining again with it would spend another try.
    public private(set) var refused = false
    /// The presenter and the code it joined with, which `again` joins with.
    @ObservationIgnored private let endpoint: NWEndpoint
    @ObservationIgnored private let code: String
    @ObservationIgnored private let seconds: Double
    @ObservationIgnored private var link: RemoteLink?

    /// Join the presenter at `endpoint` with `code`, giving up after `seconds`.
    public init(_ endpoint: NWEndpoint, code: String, seconds: Double = 10) {
        self.endpoint = endpoint
        self.code = code
        self.seconds = seconds
        let link = RemoteLink(endpoint, code: code, seconds: seconds)
        self.link = link
        link.start { [weak self] heard in
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.apply(heard) }
            }
        }
    }

    /// Ask the show for `command`.
    public func send(_ command: RemoteCommand) {
        send(RemoteMessage(command: command))
    }

    /// Ask the show for the slide numbered `number`, as typing its number on the presenter's keys
    /// does (PLAN 2.105, lift-up FUNC-5).
    public func go(slide number: Int) {
        guard number > 0 else { return }
        send(RemoteMessage(slide: number))
    }

    private func send(_ message: RemoteMessage) {
        guard status == .joined, let body = try? JSONEncoder().encode(message) else { return }
        link?.send(body)
    }

    /// A remote that joins the same presenter again with the same code, this one left: what a
    /// remote lost mid-talk does, so its code is never typed again on stage (lift-up 20261110
    /// REL-4).
    public func again() -> RemoteClient {
        leave()
        return RemoteClient(endpoint, code: code, seconds: seconds)
    }

    /// Leave the presenter.
    public func leave() {
        link?.leave()
    }

    /// What the link heard, said.
    private func apply(_ heard: RemoteLink.Heard) {
        if case .lost = status { return }
        switch heard {
        case .joined:
            status = .joined
        case .place(let place):
            self.place = place
        case .reconnecting(let waiting):
            if status == .joined { reconnecting = waiting }
        case .lost(let why, let joined, let proving):
            rejoinable = joined
            refused = proving && !joined
            reconnecting = false
            status = .lost(why)
        }
    }
}

/// A remote's connection to its presenter, worked on a queue of its own: the pairing, the proofs,
/// and the sealed channel, every one of them touched on `queue` alone. What it hears it hands on in
/// turn, as `Heard`; it gives up waiting for the pairing after its patience, which a busy main
/// actor does not spend.
final class RemoteLink: @unchecked Sendable {
    /// What the link hears.
    enum Heard: Sendable {
        case joined
        case place(RemotePlace)
        /// The network gone while joined, or back.
        case reconnecting(Bool)
        /// It could not join, or the presenter went: why, whether it had joined, and whether it
        /// had proved itself, a presenter that let go then having had another code.
        case lost(String, joined: Bool, proving: Bool)
    }

    private let queue = DispatchQueue(label: "ScaenaRemote.RemoteClient")
    private let connection: NWConnection
    private let code: String
    private let seconds: Double
    private var hand: @Sendable (Heard) -> Void = { _ in }
    /// This end of the pairing, until paired.
    private var pairing: Pairing?
    /// The keys, once both hellos have passed.
    private var keys: Pairing.Keys?
    /// Whether this end has proved itself and waits for the presenter to: a presenter that lets
    /// go then had another code.
    private var proving = false
    /// The channel, once paired.
    private var sealed: Sealed?
    private var joined = false
    private var lost = false
    /// Whether the remote left, so its patience says nothing.
    private var left = false

    init(_ endpoint: NWEndpoint, code: String, seconds: Double) {
        connection = NWConnection(to: endpoint, using: Remote.parameters())
        self.code = code
        self.seconds = seconds
    }

    /// Pair and join, handing what it hears to `hand`; given up after its `seconds`.
    func start(_ hand: @escaping @Sendable (Heard) -> Void) {
        queue.async { [self] in
            self.hand = hand
            pairing = try? Pairing(side: .remote, code: code)
            connection.stateUpdateHandler = { [weak self] state in self?.changed(state) }
            connection.start(queue: queue)
            queue.asyncAfter(deadline: .now() + seconds) { [weak self] in
                guard let self, !self.joined, !self.left else { return }
                self.lose("No answer with that code.")
            }
        }
    }

    /// `body` sent sealed, once joined.
    func send(_ body: Data) {
        queue.async { [self] in
            guard joined, !lost, let sealed, let frame = try? sealed.seal(body) else { return }
            connection.send(frame: frame)
        }
    }

    /// Leave the presenter.
    func leave() {
        queue.async { [self] in
            left = true
            connection.cancel()
        }
    }

    private func changed(_ state: NWConnection.State) {
        switch state {
        case .ready:
            // Back from waiting for the network, already joined: the show goes on as it was.
            if joined {
                hand(.reconnecting(false))
                return
            }
            hello()
        case .waiting:
            if joined { hand(.reconnecting(true)) }
        case .failed:
            // What the network says of it is not a person's words (PLAN 3.18). A presenter that
            // lets go once this end has proved itself had another code.
            lose(proving ? "No answer with that code." : "The presenter can't be reached. Check that this device is on its network, and try again.")
        case .cancelled:
            if !joined { lose(proving ? "No answer with that code." : "Not joined.") }
        default:
            break
        }
    }

    /// Send this end's hello, and hear the presenter's.
    private func hello() {
        guard let pairing, let body = try? JSONEncoder().encode(pairing.hello) else {
            lose("Not joined.")
            return
        }
        connection.send(frame: body)
        connection.receiveFrame(largest: Remote.small) { [weak self] body in
            self?.answered(body)
        }
    }

    /// The presenter's hello: the keys from it, this end proved, and the presenter's proof heard.
    private func answered(_ body: Data?) {
        guard let pairing, let body, let hello = try? JSONDecoder().decode(Pairing.Hello.self, from: body),
            let keys = try? pairing.keys(from: hello), let proof = try? JSONEncoder().encode(Pairing.Proof(proof: keys.tag(.remote)))
        else {
            lose("No answer from the presenter. Try again in a moment.")
            return
        }
        self.keys = keys
        self.pairing = nil
        proving = true
        connection.send(frame: proof)
        connection.receiveFrame(largest: Remote.small) { [weak self] body in
            self?.proved(body)
        }
    }

    /// The presenter's proof: joined, the channel sealed, and each place heard.
    private func proved(_ body: Data?) {
        guard !lost, let keys, let body, let proof = try? JSONDecoder().decode(Pairing.Proof.self, from: body),
            keys.proves(.presenter, proof.proof)
        else {
            lose("No answer with that code.")
            return
        }
        sealed = Sealed(keys: keys, side: .remote)
        self.keys = nil
        joined = true
        hand(.joined)
        hearPlaces()
    }

    /// Each place the presenter sends, sealed, in turn; lost at the first that is not.
    private func hearPlaces() {
        connection.receiveFrame(largest: Remote.largest) { [weak self] frame in
            guard let self else { return }
            guard let frame, let sealed = self.sealed, let body = try? sealed.open(frame),
                let message = try? JSONDecoder().decode(RemoteMessage.self, from: body)
            else {
                self.lose("The presenter went.")
                return
            }
            if let place = message.place { self.hand(.place(place.received())) }
            self.hearPlaces()
        }
    }

    private func lose(_ why: String) {
        guard !lost else { return }
        lost = true
        hand(.lost(why, joined: joined, proving: proving))
        connection.cancel()
    }
}
