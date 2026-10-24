import Foundation
import Network
@testable import ScaenaRemote
import Testing

/// What the presenter heard.
@MainActor
private final class Heard {
    var asked: [RemoteCommand] = []
    var joined = 0
    /// Each code made anew after wrong ones.
    var recoded: [String] = []
    /// How many wrong codes were tried, as last told.
    var tried = 0
}

/// A presenter on the loopback with `code`, what it hears kept, listening.
@MainActor
private func presenter(_ code: String, cooldown: Double = 5) async throws -> (RemoteServer, Heard) {
    let server = try RemoteServer(name: nil, code: code, loopback: true, cooldown: cooldown)
    let heard = Heard()
    server.command = { heard.asked.append($0) }
    server.joined = { heard.joined = $0 }
    server.recoded = { heard.recoded.append($0) }
    server.tried = { heard.tried = $0 }
    #expect(await waited { server.port != nil }, "the presenter listens")
    return (server, heard)
}

/// Whether `remote` was lost, and why.
@MainActor
private func lost(_ remote: RemoteClient) -> String? {
    if case .lost(let why) = remote.status { why } else { nil }
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
private func here(_ port: UInt16, host: NWEndpoint.Host = "127.0.0.1") -> NWEndpoint {
    .hostPort(host: host, port: NWEndpoint.Port(rawValue: port)!)
}

/// The states a connection went through.
@MainActor
private final class Seen {
    var states: [String] = []
}

/// What became of a connection of the test's own to `endpoint` with `parameters` within `seconds`:
/// each state it went through, in words. Where a remote does not join, it says why.
@MainActor
private func tried(_ endpoint: NWEndpoint, _ parameters: NWParameters, _ seconds: Double = 5) async -> String {
    let seen = Seen()
    let connection = NWConnection(to: endpoint, using: parameters)
    connection.stateUpdateHandler = { state in
        MainActor.assumeIsolated { seen.states.append("\(state)") }
    }
    connection.start(queue: .main)
    _ = await waited(seconds) { seen.states.last == "ready" }
    connection.cancel()
    return seen.states.joined(separator: " → ")
}

/// A remote joins a presenter with its code, here over the loopback (PLAN 4.10): it hears where the
/// show is as it joins and as it goes, and the presenter hears what it asks. A remote with another
/// code is never joined, and nothing it asks is heard.
@MainActor @Test func aRemoteJoinsWithTheCodeAndDrivesTheShow() async throws {
    let (server, heard) = try await presenter("4821")
    let port = try #require(server.port)
    let cover = RemotePlace(
        deck: "B1", showing: true, state: "cover", index: 0, count: 40, notes: "Begin with why.", slide: Data([1, 2, 3]))
    server.tell(cover)

    let remote = RemoteClient(here(port), code: "4821")
    guard await waited(10, { remote.status == .joined }) else {
        // Why not, as connections of the test's own find the presenter: TCP alone to either
        // loopback address.
        let v4 = await tried(here(port), .tcp)
        let v6 = await tried(here(port, host: "::1"), .tcp)
        let why = "the remote joins with the code: \(remote.status); TCP to 127.0.0.1: \(v4); TCP to ::1: \(v6)"
        Issue.record("\(why)")
        remote.leave()
        server.stop()
        return
    }
    #expect(await waited { remote.place == cover }, "it hears where the show is as it joins")
    #expect(await waited { heard.joined == 1 }, "the presenter counts it")
    remote.send(.on)
    #expect(await waited { heard.asked == [.on] }, "the presenter hears what it asks")
    let goal = RemotePlace(deck: "B1", showing: true, state: "goal", index: 1, count: 40, ms: 120, rest: false)
    server.tell(goal)
    #expect(await waited { remote.place == goal }, "it hears each place told after")

    // Another code is never joined, and is one wrong try (lift-up SEC-5).
    let stranger = RemoteClient(here(port), code: "1234", seconds: 5)
    #expect(await waited { lost(stranger) != nil }, "another code is never joined: \(stranger.status)")
    #expect(lost(stranger) == "No answer with that code.")
    stranger.send(.end)
    #expect(stranger.place == nil)
    #expect(heard.asked == [.on], "nothing it asks is heard")
    #expect(await waited { heard.tried == 1 }, "the presenter counts the wrong code")
    #expect(heard.recoded.isEmpty && server.code == "4821", "one wrong code leaves the code")
    // The remote joined with the code stays, and hears on.
    let next = RemotePlace(deck: "B1", showing: true, state: "next", index: 2, count: 40)
    server.tell(next)
    #expect(await waited { remote.place == next }, "the remote joined stays joined")

    remote.leave()
    server.stop()
}

/// After five wrong codes the presenter makes its code anew and says so (lift-up SEC-5): a remote
/// joined stays, no remote pairs for a moment, and then the new code joins, so a code can be guessed
/// neither offline nor online.
@MainActor @Test func wrongCodesChangeTheCodeAndAJoinedRemoteStays() async throws {
    let (server, heard) = try await presenter("4821", cooldown: 1.5)
    let port = try #require(server.port)
    let remote = RemoteClient(here(port), code: "4821")
    guard await waited(10, { remote.status == .joined }) else {
        Issue.record("the remote joins with the code: \(remote.status)")
        remote.leave()
        server.stop()
        return
    }
    for wrong in ["0000", "1111", "2222", "3333"] {
        let stranger = RemoteClient(here(port), code: wrong, seconds: 5)
        #expect(await waited { lost(stranger) != nil }, "\(wrong) is never joined")
    }
    #expect(await waited { heard.tried == 4 }, "four wrong codes are counted: \(heard.tried)")
    #expect(heard.recoded.isEmpty && server.code == "4821", "four wrong codes leave the code")
    let fifth = RemoteClient(here(port), code: "4444", seconds: 5)
    #expect(await waited { lost(fifth) != nil }, "the fifth is never joined")
    #expect(await waited { heard.recoded.count == 1 }, "the fifth wrong code makes the code anew")
    let code = server.code
    #expect(heard.recoded == [code] && code != "4821" && code.count == 4 && code.allSatisfy(\.isNumber), "\(code)")
    #expect(heard.tried == 0, "the count begins again with the new code")
    // The remote joined stays joined.
    let goal = RemotePlace(deck: "B1", showing: true, state: "goal", index: 1, count: 40)
    server.tell(goal)
    #expect(await waited { remote.place == goal }, "the remote joined stays joined")
    #expect(remote.status == .joined)
    // No remote pairs for a moment, the new code included; after it, the new code joins and the
    // old one does not.
    let early = RemoteClient(here(port), code: code, seconds: 5)
    #expect(await waited { lost(early) != nil }, "no remote pairs right after the code changed: \(early.status)")
    try await Task.sleep(for: .seconds(2))
    let old = RemoteClient(here(port), code: "4821", seconds: 5)
    #expect(await waited { lost(old) != nil }, "the old code is never joined again")
    let later = RemoteClient(here(port), code: code)
    #expect(await waited { later.status == .joined }, "the new code joins once the moment has passed: \(later.status)")
    #expect(await waited { later.place == goal }, "and hears where the show is")

    remote.leave()
    later.leave()
    server.stop()
}

/// A proof that comes after the code was made anew, its keys made under the old code, is no try
/// and never pairs (lift-up SEC-5): the pairings let go at the fifth wrong code take their keys
/// with them at once, so the guesses at one code stay five, not one for each pairing open. The
/// proofs are handed over in turn, the right one last, as the loopback cannot order them.
@MainActor @Test func aProofLateForTheCodeMadeAnewIsNoTry() throws {
    let server = try RemoteServer(name: nil, code: "4821", loopback: true)
    let heard = Heard()
    server.recoded = { heard.recoded.append($0) }
    server.tried = { heard.tried = $0 }
    var ids: [ObjectIdentifier] = []
    var proofs: [Data] = []
    for code in ["0000", "1111", "2222", "3333", "4444", "4821"] {
        let remote = try Pairing(side: .remote, code: code)
        let presenter = try Pairing(side: .presenter, code: "4821")
        let connection = NWConnection(to: here(9), using: Remote.parameters(loopback: true))
        ids.append(server.pairing(connection, keys: try presenter.keys(from: remote.hello)))
        let tag = try remote.keys(from: presenter.hello).tag(.remote)
        proofs.append(try JSONEncoder().encode(Pairing.Proof(proof: tag)))
    }
    for (id, proof) in zip(ids, proofs).prefix(5) { server.heard(id, proof: proof) }
    #expect(heard.recoded.count == 1 && heard.tried == 0, "the fifth wrong code makes the code anew")
    server.heard(ids[5], proof: proofs[5])
    #expect(server.paired == 0, "the old code's right proof, late, pairs nothing")
    #expect(heard.tried == 0 && heard.recoded.count == 1, "and is no try")
    server.stop()
}

/// A connection that speaks no pairing is let go and counts as no try (lift-up SEC-5): a port
/// scanner, or a remote of another version, cannot change the code.
@MainActor @Test func aConnectionWithoutThePairingIsNoTry() async throws {
    let (server, heard) = try await presenter("4821")
    let port = try #require(server.port)
    let seen = Seen()
    let connection = NWConnection(to: here(port), using: .tcp)
    connection.stateUpdateHandler = { state in
        MainActor.assumeIsolated { seen.states.append("\(state)") }
    }
    connection.start(queue: .main)
    #expect(await waited { seen.states.last == "ready" }, "TCP connects: \(seen.states)")
    // A frame that is no hello.
    connection.send(frame: Data("{\"hello\": \"world\"}".utf8))
    let closed = Seen()
    connection.receiveFrame { body in
        MainActor.assumeIsolated { closed.states.append(body == nil ? "closed" : "answered") }
    }
    #expect(await waited { closed.states == ["closed"] }, "it is let go: \(closed.states)")
    #expect(heard.tried == 0 && heard.recoded.isEmpty && server.code == "4821", "and is no try at the code")
    connection.cancel()
    // The code still joins.
    let remote = RemoteClient(here(port), code: "4821")
    #expect(await waited { remote.status == .joined }, "\(remote.status)")
    remote.leave()
    server.stop()
}

/// A join recorded on the network tells nothing of the code (lift-up SEC-5): what each end sends
/// first is a point that is random whatever the code, and only the ends that began with one code
/// prove themselves to each other.
@Test func aPairingTellsNothingOfTheCodeAndProvesOnlyItsOwn() throws {
    let remote = try Pairing(side: .remote, code: "4821")
    let presenter = try Pairing(side: .presenter, code: "4821")
    let again = try Pairing(side: .remote, code: "4821")
    #expect(remote.hello.key.count == 32 && remote.hello.nonce.count == 16 && remote.hello.version == Remote.version)
    #expect(remote.hello.key != again.hello.key, "the same code sends another point each time")
    #expect(remote.hello.key != Pairing.point("4821"), "and never the code's own point")
    let ofRemote = try remote.keys(from: presenter.hello)
    let ofPresenter = try presenter.keys(from: remote.hello)
    #expect(ofPresenter.proves(.remote, ofRemote.tag(.remote)), "the ends with one code prove themselves")
    #expect(ofRemote.proves(.presenter, ofPresenter.tag(.presenter)))
    // Each end's tag is its own: one sent back to the end that made it proves nothing of the other.
    #expect(!ofRemote.proves(.presenter, ofRemote.tag(.remote)), "each end's tag is its own")
    #expect(!ofPresenter.proves(.remote, ofPresenter.tag(.presenter)))
    // Another code: the hellos pass alike, and neither end proves itself.
    let stranger = try Pairing(side: .remote, code: "1234")
    let ofStranger = try stranger.keys(from: presenter.hello)
    let ofPresenterToStranger = try presenter.keys(from: stranger.hello)
    #expect(!ofPresenterToStranger.proves(.remote, ofStranger.tag(.remote)), "another code proves nothing")
    #expect(!ofStranger.proves(.presenter, ofPresenterToStranger.tag(.presenter)))
    // A hello of another version, or a point of low order, pairs with nothing.
    var other = presenter.hello
    other.version = Remote.version + 1
    #expect(throws: Pairing.Trouble.self) { try remote.keys(from: other) }
    var low = presenter.hello
    low.key = Data(repeating: 0, count: 32)
    #expect(throws: (any Error).self) { try remote.keys(from: low) }
    // The channel: each message sealed in turn, read by the other end in turn, and no other.
    let sending = Sealed(keys: ofRemote, side: .remote)
    let receiving = Sealed(keys: ofPresenter, side: .presenter)
    let first = try sending.seal(Data("on".utf8))
    let second = try sending.seal(Data("back".utf8))
    #expect(first != Data("on".utf8) && first.range(of: Data("on".utf8)) == nil, "nothing passes in the clear")
    #expect(throws: (any Error).self, "out of turn") { try receiving.open(second) }
    #expect(try receiving.open(first) == Data("on".utf8))
    #expect(throws: (any Error).self, "replayed") { try receiving.open(first) }
    #expect(try receiving.open(second) == Data("back".utf8))
    let elsewhere = Sealed(keys: ofRemote, side: .remote)
    #expect(throws: (any Error).self, "the other way's key") { try elsewhere.open(try sending.seal(Data("end".utf8))) }
}

/// What a presenter offers its deck as is its device's name alone (lift-up SEC-5), within what a
/// Bonjour name may hold, never the deck's file name.
@Test func theServiceNameIsTheDeviceAlone() {
    #expect(Remote.serviceName(device: "Jay's MacBook Pro") == "Jay's MacBook Pro")
    #expect(Remote.serviceName(device: "  ") == "Scaena")
    let long = String(repeating: "é", count: 40)
    let cut = Remote.serviceName(device: long)
    #expect(cut.utf8.count <= 63 && cut.utf8.count > 60 && long.hasPrefix(cut), "\(cut.utf8.count) bytes")
}

/// A code is four digits, as a presentation app's remote pairs with.
@Test func aCodeIsFourDigits() {
    for _ in 0..<20 {
        let code = Remote.code()
        #expect(code.count == 4 && code.allSatisfy(\.isNumber), "\(code)")
    }
}
