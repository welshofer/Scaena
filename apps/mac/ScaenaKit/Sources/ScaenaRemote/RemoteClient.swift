import Foundation
import Network
import Observation

/// A presenter offering its deck on the local network (PLAN 4.10), as a remote finds it.
public struct RemotePresenter: Identifiable, Hashable, Sendable {
    /// What the presenter calls itself: its device, and the deck.
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
/// message after is sealed.
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
    private let connection: NWConnection
    /// This end of the pairing, until paired.
    @ObservationIgnored private var pairing: Pairing?
    /// The keys, once both hellos have passed.
    @ObservationIgnored private var keys: Pairing.Keys?
    /// Whether this end has proved itself and waits for the presenter to: a presenter that lets
    /// go then had another code.
    @ObservationIgnored private var proving = false
    /// The channel, once paired.
    @ObservationIgnored private var sealed: Sealed?
    /// Given up waiting for the pairing: a presenter that does not answer leaves it waiting.
    @ObservationIgnored private var patience: Task<Void, Never>?

    /// Join the presenter at `endpoint` with `code`, giving up after `seconds`.
    public init(_ endpoint: NWEndpoint, code: String, seconds: Double = 10) {
        connection = NWConnection(to: endpoint, using: Remote.parameters())
        pairing = try? Pairing(side: .remote, code: code)
        connection.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated { self?.changed(state) }
        }
        connection.start(queue: .main)
        patience = Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            guard !Task.isCancelled, let self, self.status == .joining else { return }
            self.lose("No answer with that code.")
        }
    }

    /// Ask the show for `command`.
    public func send(_ command: RemoteCommand) {
        guard status == .joined, let sealed, let body = try? JSONEncoder().encode(RemoteMessage(command: command)),
            let frame = try? sealed.seal(body)
        else { return }
        connection.send(frame: frame)
    }

    /// Leave the presenter.
    public func leave() {
        patience?.cancel()
        connection.cancel()
    }

    private func changed(_ state: NWConnection.State) {
        switch state {
        case .ready:
            hello()
        case .failed:
            // What the network says of it is not a person's words (PLAN 3.18). A presenter that
            // lets go once this end has proved itself had another code.
            lose(proving ? "No answer with that code." : "The presenter can't be reached. Check that this device is on its network, and try again.")
        case .cancelled:
            if status != .joined { lose(proving ? "No answer with that code." : "Not joined.") }
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
        connection.receiveFrame { [weak self] body in
            MainActor.assumeIsolated { self?.answered(body) }
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
        connection.receiveFrame { [weak self] body in
            MainActor.assumeIsolated { self?.proved(body) }
        }
    }

    /// The presenter's proof: joined, the channel sealed, and each place heard.
    private func proved(_ body: Data?) {
        guard let keys, let body, let proof = try? JSONDecoder().decode(Pairing.Proof.self, from: body),
            keys.proves(.presenter, proof.proof)
        else {
            lose("No answer with that code.")
            return
        }
        sealed = Sealed(keys: keys, side: .remote)
        self.keys = nil
        patience?.cancel()
        status = .joined
        hearPlaces()
    }

    /// Each place the presenter sends, sealed, in turn; lost at the first that is not.
    private func hearPlaces() {
        connection.receiveFrame { [weak self] frame in
            MainActor.assumeIsolated {
                guard let self else { return }
                guard let frame, let sealed = self.sealed, let body = try? sealed.open(frame),
                    let message = try? JSONDecoder().decode(RemoteMessage.self, from: body)
                else {
                    self.lose("The presenter went.")
                    return
                }
                if let place = message.place { self.place = place }
                self.hearPlaces()
            }
        }
    }

    private func lose(_ why: String) {
        patience?.cancel()
        if case .lost = status { return }
        status = .lost(why)
        connection.cancel()
    }
}
