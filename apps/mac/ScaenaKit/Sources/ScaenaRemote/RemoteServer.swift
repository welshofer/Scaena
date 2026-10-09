import Foundation
import Network

/// The presenter's end of the remote (PLAN 4.10): it listens for remotes that have its code, under
/// Bonjour where it is given a name, tells each where the show is as it goes, and hands on what
/// each asks. Its work is done on the main queue.
@MainActor
public final class RemoteServer {
    /// The code a remote joins with: shown where the audience does not see it, in the editor.
    public let code: String
    /// The port it listens on, once it does.
    public var port: UInt16? { listener.port?.rawValue }
    /// What a remote asks.
    public var command: (RemoteCommand) -> Void = { _ in }
    /// How many remotes are joined, as it changes.
    public var joined: (Int) -> Void = { _ in }
    /// Why it stopped listening, where it did.
    public var failed: (String) -> Void = { _ in }

    private let listener: NWListener
    private var remotes: [ObjectIdentifier: NWConnection] = [:]
    private var place: RemotePlace?

    /// Listen with `code`, offered by Bonjour as `name` on the local network; with no name, only to
    /// one who knows the port; with `loopback`, only on this machine, as a test listens.
    public init(name: String?, code: String = Remote.code(), loopback: Bool = false) throws {
        self.code = code
        let parameters = Remote.parameters(code: code)
        if loopback { parameters.requiredInterfaceType = .loopback }
        listener = try NWListener(using: parameters)
        if let name { listener.service = NWListener.Service(name: name, type: Remote.service) }
        listener.newConnectionHandler = { [weak self] connection in
            MainActor.assumeIsolated { self?.join(connection) }
        }
        listener.stateUpdateHandler = { [weak self] state in
            guard case .failed(let error) = state else { return }
            MainActor.assumeIsolated { self?.failed("\(error)") }
        }
        listener.start(queue: .main)
    }

    /// Tell every remote where the show is, and each that joins from now.
    public func tell(_ place: RemotePlace) {
        self.place = place
        for remote in remotes.values where remote.state == .ready {
            remote.send(RemoteMessage(place: place))
        }
    }

    /// Stop listening, and let every remote go.
    public func stop() {
        listener.cancel()
        for remote in remotes.values { remote.cancel() }
        remotes = [:]
        joined(0)
    }

    private func join(_ connection: NWConnection) {
        let id = ObjectIdentifier(connection)
        remotes[id] = connection
        connection.stateUpdateHandler = { [weak self, weak connection] state in
            MainActor.assumeIsolated {
                guard let self, let connection else { return }
                switch state {
                case .ready:
                    self.joined(self.count)
                    if let place = self.place { connection.send(RemoteMessage(place: place)) }
                case .failed, .cancelled:
                    self.remotes[id] = nil
                    self.joined(self.count)
                default:
                    break
                }
            }
        }
        connection.receiveMessages { [weak self, weak connection] message in
            MainActor.assumeIsolated {
                guard let message else {
                    connection?.cancel()
                    return
                }
                if let command = message.command { self?.command(command) }
            }
        }
        connection.start(queue: .main)
    }

    /// The remotes joined.
    private var count: Int { remotes.values.filter { $0.state == .ready }.count }
}
