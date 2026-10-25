import Foundation
import Network
@testable import ScaenaRemote
import Testing

/// What the presenter heard.
@MainActor
private final class Heard {
    var asked: [RemoteCommand] = []
    /// Each slide asked for by its number.
    var slides: [Int] = []
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
    server.slide = { heard.slides.append($0) }
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

/// The remote sees what the room sees (lift-up FUNC-5): a black or white screen, the end of the
/// show, and the presenter's timer, paused where it is; and it asks for them, and for a slide by
/// its number, as the presenter's keys do.
@MainActor @Test func aRemoteMirrorsTheScreenTheEndAndTheTimerAndAsksForThem() async throws {
    let (server, heard) = try await presenter("5307")
    let port = try #require(server.port)
    let at = Date(timeIntervalSince1970: 1_790_000_000)
    let blanked = RemotePlace(
        deck: "B1", showing: true, state: "goal", index: 1, count: 40, blank: .black,
        timer: RemoteTimer(elapsed: 75, paused: true, at: at))
    server.tell(blanked)
    let remote = RemoteClient(here(port), code: "5307")
    guard await waited(10, { remote.status == .joined }) else {
        Issue.record("the remote joins with the code: \(remote.status)")
        remote.leave()
        server.stop()
        return
    }
    // Its timer is stamped as it arrives, by the remote's clock; the rest is as the presenter told it.
    #expect(await waited { remote.place?.timer != nil }, "it hears the place")
    let heardTimer = try #require(remote.place?.timer)
    #expect(remote.place == blanked.received(at: heardTimer.at), "it hears the screen black")
    #expect(heardTimer.elapsed == 75 && heardTimer.paused, "and the timer paused where it stood")
    let ended = RemotePlace(deck: "B1", showing: true, state: "close", index: 39, count: 40, over: true)
    server.tell(ended)
    #expect(await waited { remote.place?.over == true && remote.place?.blank == nil }, "and the end of the show")

    remote.send(.white)
    remote.send(.pauseTimer)
    remote.send(.resetTimer)
    remote.go(slide: 12)
    remote.go(slide: 0)
    #expect(await waited { heard.asked == [.white, .pauseTimer, .resetTimer] }, "the presenter hears each")
    #expect(await waited { heard.slides == [12] }, "and the slide by its number, never one numbered 0")

    remote.leave()
    server.stop()
}

/// A presenter from before the remote mirrored the room sends no screen, end, or timer: its place
/// reads with none, the time since the show began standing in for the timer.
@Test func aPlaceFromAnOlderPresenterReadsWithNoScreenEndOrTimer() throws {
    let sent = #"{"deck":"B1","showing":true,"state":"cover","index":0,"count":40,"ms":0,"rest":true,"notes":""}"#
    let place = try JSONDecoder().decode(RemotePlace.self, from: Data(sent.utf8))
    #expect(place.blank == nil && !place.over && place.timer == nil)
    let told = RemotePlace(
        deck: "B1", showing: true, blank: .white, over: false, timer: RemoteTimer(elapsed: 3, paused: false, at: .now))
    let back = try JSONDecoder().decode(RemotePlace.self, from: JSONEncoder().encode(told))
    #expect(back.blank == .white && back.timer?.elapsed == 3, "what a presenter tells reads back as told")
}

/// A remote whose clock is off from the presenter's reads the presenter's time all the same: the
/// timer is stamped as the place arrives, by the remote's own clock.
@Test func aRemoteWithAClockOffReadsThePresentersTime() {
    let here = Date(timeIntervalSince1970: 1_790_000_000)
    // The presenter's clock 30 s ahead of the remote's.
    let told = RemotePlace(
        deck: "B1", showing: true, timer: RemoteTimer(elapsed: 75, paused: false, at: here.addingTimeInterval(30)))
    #expect(told.timer?.reads(at: here.addingTimeInterval(10)) == "1:15", "read by the presenter's stamp, it is 10 s short")
    let heard = told.received(at: here)
    #expect(heard.timer?.reads(at: here) == "1:15", "taken as it arrives, it reads what the presenter's view reads")
    #expect(heard.timer?.reads(at: here.addingTimeInterval(10)) == "1:25", "and runs on from there")
    #expect(RemotePlace(deck: "B1", showing: true).received(at: here).timer == nil, "a place with no timer takes none")
}

/// The presenter's timer reads on the remote as on the presenter's view: paused where it stands,
/// else running on from when it was told.
@Test func theTimerReadsAsThePresentersDoes() {
    let at = Date(timeIntervalSince1970: 1_790_000_000)
    let paused = RemoteTimer(elapsed: 125, paused: true, at: at)
    #expect(paused.reads(at: at.addingTimeInterval(60)) == "2:05", "paused, it stands")
    let running = RemoteTimer(elapsed: 125, paused: false, at: at)
    #expect(running.reads(at: at.addingTimeInterval(60)) == "3:05", "running, it runs on")
    #expect(running.reads(at: at.addingTimeInterval(-30)) == "2:05", "never back for a clock behind")
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

/// A join leaves the wrong codes counted (lift-up SEC-5): they are counted since the code was made,
/// so four wrong, then a join, then one more makes the code anew. A join bought no more tries.
@MainActor @Test func aJoinLeavesTheWrongCodesCounted() async throws {
    let (server, heard) = try await presenter("4821", cooldown: 1.5)
    let port = try #require(server.port)
    for wrong in ["0000", "1111", "2222", "3333"] {
        let stranger = RemoteClient(here(port), code: wrong, seconds: 5)
        #expect(await waited { lost(stranger) != nil }, "\(wrong) is never joined")
    }
    #expect(await waited { heard.tried == 4 }, "four wrong codes are counted: \(heard.tried)")
    let remote = RemoteClient(here(port), code: "4821")
    #expect(await waited { remote.status == .joined }, "the code joins: \(remote.status)")
    #expect(heard.recoded.isEmpty && server.code == "4821")
    let fifth = RemoteClient(here(port), code: "4444", seconds: 5)
    #expect(await waited { lost(fifth) != nil }, "the fifth is never joined")
    #expect(await waited { heard.recoded.count == 1 }, "the fifth wrong code, after the join, makes the code anew")
    #expect(server.code != "4821")
    remote.leave()
    server.stop()
}

/// Before it pairs, a presenter holds no frame of more than `Remote.small` (lift-up SEC-5): one
/// that says it is longer is let go unread, and is no try at the code.
@MainActor @Test func aFrameTooLongToPairIsLetGo() async throws {
    let (server, heard) = try await presenter("4821")
    let port = try #require(server.port)
    let seen = Seen()
    let connection = NWConnection(to: here(port), using: .tcp)
    connection.stateUpdateHandler = { state in
        MainActor.assumeIsolated { seen.states.append("\(state)") }
    }
    connection.start(queue: .main)
    #expect(await waited { seen.states.last == "ready" }, "TCP connects: \(seen.states)")
    connection.send(frame: Data(repeating: 0x20, count: Remote.small + 1))
    let closed = Seen()
    connection.receiveFrame(largest: Remote.small) { body in
        MainActor.assumeIsolated { closed.states.append(body == nil ? "closed" : "answered") }
    }
    #expect(await waited { closed.states == ["closed"] }, "it is let go: \(closed.states)")
    #expect(heard.tried == 0 && heard.recoded.isEmpty && server.code == "4821", "and is no try at the code")
    connection.cancel()
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
    var ids: [Int] = []
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
    connection.receiveFrame(largest: Remote.small) { body in
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
    #expect(remote.hello.key != Pairing.x(try Pairing.base("4821")), "and never the code's own point")
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
    // A hello of another version, or an x of no point of the curve, pairs with nothing: one past
    // the field's prime, and 1, which no point has.
    var other = presenter.hello
    other.version = Remote.version + 1
    #expect(throws: Pairing.Trouble.self) { try remote.keys(from: other) }
    for x in [Data(repeating: 0xff, count: 32), Data(repeating: 0, count: 31) + Data([1])] {
        var none = presenter.hello
        none.key = x
        #expect(throws: Pairing.Trouble.self) { try remote.keys(from: none) }
    }
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

/// Every code of four digits has its point on P-256 (lift-up SEC-5), so no hello says which codes
/// it could be; and the map from a code to its point is pinned, so no change to it goes unnoticed:
/// each point's x, as SHA-256 of the label, the code, and the first counter that gives a point.
@Test func everyCodeHasItsPointOnTheCurve() throws {
    let pinned = [
        "0000": "be08c668dc503304e72a59c263a547ceb036013a36ed270d74586beae146a5e7",
        "1111": "eeb07914c80dde656064cc95a8faf91bd03bcb456034dfbed51a359c111e8d72",
        "9999": "c29c84114b4e092152eb6af7d71a7877b6250d95ced934677026eef1d54c679e",
    ]
    for (code, x) in pinned {
        let made = Pairing.x(try Pairing.base(code)).map { String(format: "%02x", $0) }.joined()
        #expect(made == x, "\(code)'s point")
    }
    for n in 0..<10_000 {
        let code = String(format: "%04d", n)
        #expect((try? Pairing.base(code)) != nil, "\(code) has its point")
    }
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

/// The remote counts the room's slides, not the show's states (lift-up USE-2): a step after a
/// slide's first is that slide's step, and a presenter that counts no slides is read as before.
@Test func aRemoteCountsSlidesAndTheirSteps() throws {
    // Three slides, the second of three steps: the states a, b, b, b, c.
    let of = ["a", "b", "b", "b", "c"]
    let counts = (0..<5).compactMap { RemotePlace.counted(of, at: $0) }.map { [$0.number, $0.slides, $0.step] }
    #expect(counts == [[1, 3, 1], [2, 3, 1], [2, 3, 2], [2, 3, 3], [3, 3, 1]])
    #expect(RemotePlace.counted(of, at: 5) == nil, "no slide past the last state")
    let step = RemotePlace(deck: "B1", showing: true, state: "b2", index: 2, count: 5, number: 2, slides: 3, step: 2)
    #expect(step.said == "Slide 2 of 3, step 2")
    let first = RemotePlace(deck: "B1", showing: true, state: "c", index: 4, count: 5, number: 3, slides: 3, step: 1)
    #expect(first.said == "Slide 3 of 3")
    let back = try JSONDecoder().decode(RemotePlace.self, from: JSONEncoder().encode(step))
    #expect(back == step && back.said == "Slide 2 of 3, step 2", "a place sent says its slide and step")
    // A presenter from before sends neither: the remote counts the states, as it did.
    let older = #"{"deck":"B1","showing":true,"state":"goal","index":6,"count":19,"ms":0,"rest":true,"notes":""}"#
    let place = try JSONDecoder().decode(RemotePlace.self, from: Data(older.utf8))
    #expect(place.number == nil && place.said == "7 of 19")
}
