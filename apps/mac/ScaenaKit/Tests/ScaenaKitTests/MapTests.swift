import CoreGraphics
import Foundation
import Testing

@testable import ScaenaKit

/// What a test's Geoapify was asked, request by request.
private actor Asked {
    private(set) var requests: [URLRequest] = []

    func add(_ request: URLRequest) {
        requests.append(request)
    }

    /// How many static maps were asked for.
    var statics: Int { requests.filter { $0.url?.host == "maps.geoapify.com" }.count }
}

/// Insert › Map… (ADR-0030, PLAN O.1d), as the Mac's sheet puts one, against Geoapify's recorded
/// answers. On a slide started in Photo with nothing selected, a name that matches three places
/// offers them; the second picked, its static map is fetched, the key in each request's query
/// alone, and the map fills the empty place for a picture with its record and its pin, one patch.
/// Fetched again a step nearer, the image shows the new picture, one patch. A key Geoapify does not
/// take is said so.
@MainActor
@Test func aMapFillsItsPlaceAndIsFetchedAgainNearer() async throws {
    let maps = repository.appending(path: "tests/fixtures/maps")
    let several = try Data(contentsOf: maps.appending(path: "places-several.json"))
    let picture = try Data(contentsOf: maps.appending(path: "map.png"))
    let unauthorized = try Data(contentsOf: maps.appending(path: "unauthorized.json"))
    let asked = Asked()
    let generator = Generator { request in
        let made = try? request.urlRequest()
        if let made { await asked.add(made) }
        if request.url.contains("apiKey=refused") {
            return GeneratorReply(status: 401, kind: "application/json", body: unauthorized)
        }
        if request.url.hasPrefix("https://maps.geoapify.com/") {
            // Each picture its own: the same pixels, a byte of its own after the end.
            let n = await asked.statics
            return GeneratorReply(status: 200, kind: "image/png", body: picture + [UInt8(n % 256)])
        }
        return GeneratorReply(status: 200, kind: "application/json", body: several)
    }
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Maps"))
    let session = editor.session
    let started = try session.starting(after: "start", layout: "photo")
    try editor.make(started.patch)

    // Nothing selected: the slide's empty place for a picture, in Dusk's dark style.
    let target = try session.mapTarget(state: started.id, selected: [], pressed: nil)
    #expect(target.slot == "art" && target.node == nil && target.style == "dark" && target.state == started.id)

    // Three places offered; the second picked, its map fetched.
    let offered = try await generator.map(MapAsk(place: "Springfield", show: .city, target: target), key: "geo-key")
    guard case .places(let places) = offered else {
        Issue.record("three places are offered: \(offered)")
        return
    }
    #expect(places.count == 3 && places[1].address == "Springfield, MA, United States of America")
    let answer = try await generator.map(MapAsk(place: "Springfield", show: .city, target: target, pick: 1), key: "geo-key")
    guard case .map(let map) = answer else {
        Issue.record("the map is fetched: \(answer)")
        return
    }
    #expect(map.png.starts(with: picture) && map.place == "Springfield")
    let requests = await asked.requests
    #expect(requests.allSatisfy { $0.url?.query?.hasSuffix("apiKey=geo-key") == true }, "the key in each query")
    #expect(
        requests.allSatisfy { r in (r.allHTTPHeaderFields ?? [:]).values.allSatisfy { !$0.contains("geo-key") } },
        "no header holds the key")
    let statics = requests.compactMap(\.url?.absoluteString).filter { $0.hasPrefix("https://maps.geoapify.com/") }
    #expect(statics.count == 1 && statics[0].contains("lonlat:-72.5898,42.1015"))

    // Put in the empty place: one patch, the picture in the bundle, the record without the key.
    let put = try session.mapping(map, at: target, label: nil)
    #expect(put.said == "Map of Springfield from Geoapify added")
    try editor.make(put.patch)
    #expect(try session.waiting(state: started.id).isEmpty, "the place waits no more")
    let fields = try session.choices(state: started.id, node: put.node).fields
    #expect(fields.contains { $0.prop == "pin/label" }, "the inspector offers the pin's label")
    let files = { () throws -> [String] in try session.call("imageFiles") }
    let first = try #require(try files().first { $0.hasSuffix(".png") })

    // A step nearer: one static map, no geocoder, the image showing it.
    let again = try session.mapRefreshing(state: started.id, node: put.node)
    #expect(again.target.node == put.node)
    let before = await asked.requests.count
    let nearer = try await generator.map(
        MapAsk(again: again, zoom: again.zoom + 1, center: again.center), key: "geo-key")
    guard case .map(let near) = nearer else {
        Issue.record("the map is fetched again: \(nearer)")
        return
    }
    let since = await asked.requests.dropFirst(before)
    #expect(since.count == 1 && since.first?.url?.host == "maps.geoapify.com", "one static map, no geocoder")
    let replaced = try session.mapping(near, at: again.target, label: again.label ?? "")
    try editor.make(replaced.patch)
    #expect(replaced.node == put.node)
    let now = try #require(try files().first { $0.hasSuffix(".png") })
    #expect(now != first, "the image shows the nearer picture")

    // A key Geoapify does not take: said, and nothing is put anywhere.
    await #expect(throws: ScaenaError.self) {
        _ = try await generator.map(MapAsk(place: "Ferry Building", target: target), key: "refused")
    }
}

/// What a stand-in for the model answers, in order.
private actor Model {
    private var bodies: [String]

    init(_ bodies: [String]) {
        self.bodies = bodies
    }

    func answer(_ request: URLRequest) -> (Data, Int) {
        guard !bodies.isEmpty else { return (Data(#"{"error": {"message": "no more"}}"#.utf8), 500) }
        return (Data(bodies.removeFirst().utf8), 200)
    }
}

/// The assistant's `map_fetch` on the Mac (PLAN O.1d), as the browser's assistant runs it: the map
/// fetched here with the Geoapify key kept, put on B1's cover at the box the model names, by the
/// model's agent, one edit the window's undo takes back; its result the MCP tool's. With no key
/// kept, it says where one goes, and changes nothing.
@MainActor
@Test(.timeLimit(.minutes(2))) func theAssistantFetchesAMapAsInsertDoes() async throws {
    let maps = repository.appending(path: "tests/fixtures/maps")
    let several = try Data(contentsOf: maps.appending(path: "places-several.json"))
    let picture = try Data(contentsOf: maps.appending(path: "map.png"))
    let input = #"{"place": "Springfield", "pick": 2, "state": "cover", "at": [96, 96, 640, 360]}"#
    let call = #"{"type": "tool_use", "id": "t1", "name": "map_fetch", "input": \#(input)}"#
    let calling = #"{"content": [\#(call)], "stop_reason": "tool_use"}"#
    let done = #"{"content": [{"type": "text", "text": "Done."}], "stop_reason": "end_turn"}"#
    let editor = DeckEditor(session: try ScaenaSession(directory: b1))
    let model = Model([calling, done])
    let assistant = Assistant(editor: editor) { request in await model.answer(request) }
    assistant.mapKey = { "geo-key" }
    assistant.mapSend = { request in
        request.url.hasPrefix("https://maps.geoapify.com/")
            ? GeneratorReply(status: 200, kind: "image/png", body: picture)
            : GeneratorReply(status: 200, kind: "application/json", body: several)
    }
    var undone: [String] = []
    assistant.edited = { before, _ in undone.append(before) }
    let was = editor.source

    assistant.ask(
        "Put a map of Springfield, Massachusetts on the cover.", provider: .anthropic, model: "a-model", key: "sk-test")
    // Awaited, not timed: a slow simulator answers late, and a hang meets the test's time limit.
    await assistant.asking?.value
    let ran = assistant.entries.compactMap { entry -> Ran? in
        if case .call(_, _, let ran) = entry.kind { return ran }
        return nil
    }
    let fetched = try #require(ran.first)
    #expect(!fetched.error && fetched.edited, "\(fetched.json)")
    #expect(fetched.json.contains("Map of Springfield from Geoapify added"), "\(fetched.json)")
    #expect(ToolWords.ran(fetched) == "Put a map on the slide")
    #expect(undone == [was], "the window's undo was handed the source it replaced")
    #expect(editor.source.contains("geoapify"), "the map's record is in the deck")
    #expect(!editor.source.contains("geo-key"), "and no key")

    // No key kept: said, and nothing changes.
    let again = Model([calling, done])
    let keyless = Assistant(editor: editor) { request in await again.answer(request) }
    keyless.mapKey = { nil }
    let now = editor.source
    keyless.ask("Another map.", provider: .anthropic, model: "a-model", key: "sk-test")
    for _ in 0..<500 where keyless.working {
        try await Task.sleep(for: .milliseconds(10))
    }
    let refused = keyless.entries.compactMap { entry -> Ran? in
        if case .call(_, _, let ran) = entry.kind { return ran }
        return nil
    }
    #expect(refused.first?.error == true && refused.first?.json.contains("no Geoapify key") == true)
    #expect(editor.source == now)
}
