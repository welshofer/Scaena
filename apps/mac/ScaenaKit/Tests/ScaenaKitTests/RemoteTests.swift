import Foundation
import Network
import ScaenaRemote
import Testing

/// What the presenter heard.
@MainActor
private final class Heard {
    var asked: [RemoteCommand] = []
    var joined = 0
}

/// Whether `holds` comes to hold within `seconds`, asked every tenth of a second.
@MainActor
private func waited(_ seconds: Double = 10, _ holds: () -> Bool) async -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while Date() < end {
        if holds() { return true }
        try? await Task.sleep(for: .milliseconds(100))
    }
    return holds()
}

/// The presenter at `port` on this machine.
private func here(_ port: UInt16) -> NWEndpoint {
    .hostPort(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: port)!)
}

/// A remote joins a presenter with its code, here over the loopback (PLAN 4.10): it hears where the
/// show is as it joins and as it goes, and the presenter hears what it asks. A remote with another
/// code is never joined, and nothing it asks is heard.
@MainActor @Test func aRemoteJoinsWithTheCodeAndDrivesTheShow() async throws {
    let server = try RemoteServer(name: nil, code: "4821", loopback: true)
    let heard = Heard()
    server.command = { heard.asked.append($0) }
    server.joined = { heard.joined = $0 }
    #expect(await waited { server.port != nil }, "the presenter listens")
    let port = try #require(server.port)
    let cover = RemotePlace(
        deck: "B1", showing: true, state: "cover", index: 0, count: 40, notes: "Begin with why.", slide: Data([1, 2, 3]))
    server.tell(cover)

    let remote = RemoteClient(here(port), code: "4821")
    #expect(await waited { remote.status == .joined }, "the remote joins with the code: \(remote.status)")
    #expect(await waited { remote.place == cover }, "it hears where the show is as it joins")
    #expect(await waited { heard.joined == 1 }, "the presenter counts it")
    remote.send(.on)
    #expect(await waited { heard.asked == [.on] }, "the presenter hears what it asks")
    let goal = RemotePlace(deck: "B1", showing: true, state: "goal", index: 1, count: 40, ms: 120, rest: false)
    server.tell(goal)
    #expect(await waited { remote.place == goal }, "it hears each place told after")

    let stranger = RemoteClient(here(port), code: "1234", seconds: 3)
    #expect(await waited { if case .lost = stranger.status { true } else { false } }, "another code is never joined")
    stranger.send(.end)
    #expect(stranger.place == nil)
    #expect(heard.asked == [.on], "nothing it asks is heard")

    remote.leave()
    server.stop()
}

/// A code is four digits, as a presentation app's remote pairs with.
@Test func aCodeIsFourDigits() {
    for _ in 0..<20 {
        let code = Remote.code()
        #expect(code.count == 4 && code.allSatisfy(\.isNumber), "\(code)")
    }
}
