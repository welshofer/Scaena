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
    /// Why none can be found, where it is so: the local network refused, say.
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
                case .failed(let error), .waiting(let error): self?.trouble = "\(error)"
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
}

/// A remote joined to a presenter (PLAN 4.10): where the show is, as the presenter tells it, and
/// what this remote asks of it.
@MainActor @Observable
public final class RemoteClient {
    public enum Status: Equatable, Sendable {
        /// Joining: the handshake, which a wrong code fails.
        case joining
        case joined
        /// It could not join, or the presenter went: why.
        case lost(String)
    }

    public private(set) var status = Status.joining
    /// Where the show is; none until the presenter says.
    public private(set) var place: RemotePlace?
    private let connection: NWConnection
    /// Given up waiting for the handshake: a wrong code leaves it waiting.
    @ObservationIgnored private var patience: Task<Void, Never>?

    /// Join the presenter at `endpoint` with `code`, giving up after `seconds`.
    public init(_ endpoint: NWEndpoint, code: String, seconds: Double = 10) {
        connection = NWConnection(to: endpoint, using: Remote.parameters(code: code))
        connection.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated { self?.changed(state) }
        }
        connection.receiveMessages { [weak self] message in
            MainActor.assumeIsolated {
                guard let self else { return }
                guard let message else {
                    self.lose("The presenter went.")
                    return
                }
                if let place = message.place { self.place = place }
            }
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
        guard status == .joined else { return }
        connection.send(RemoteMessage(command: command))
    }

    /// Leave the presenter.
    public func leave() {
        patience?.cancel()
        connection.cancel()
    }

    private func changed(_ state: NWConnection.State) {
        switch state {
        case .ready:
            patience?.cancel()
            status = .joined
        case .failed(let error):
            lose("\(error)")
        case .cancelled:
            if status != .joined { lose("Not joined.") }
        default:
            break
        }
    }

    private func lose(_ why: String) {
        patience?.cancel()
        if case .lost = status { return }
        status = .lost(why)
        connection.cancel()
    }
}
