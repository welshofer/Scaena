import Foundation
import Testing

@testable import ScaenaKit

/// What Anthropic answered, recorded for the Rust tests too.
private let answers = repository.appending(path: "crates/scaena-narrato/tests/answers")

/// What a test's providers were asked, request by request.
private actor Asked {
    private(set) var requests: [GeneratorRequest] = []

    func add(_ request: GeneratorRequest) {
        requests.append(request)
    }
}

/// The steps a generator made away from the main actor, each with whether it was made on the
/// main thread and the stack of the thread it was made on, as that thread tells them.
private final class Away: @unchecked Sendable {
    private let lock = NSLock()
    private var taken: [(step: String, main: Bool, stack: Int)] = []

    func add(_ step: String, main: Bool, stack: Int) {
        lock.withLock { taken.append((step, main, stack)) }
    }

    var steps: [(step: String, main: Bool, stack: Int)] { lock.withLock { taken } }
}

/// An Anthropic answer whose text is `text`.
private func anthropic(_ text: String) -> Data {
    let answer: JSONValue = .object([
        "content": .array([.object(["type": .string("text"), "text": .string(text)])]),
        "stop_reason": .string("end_turn"),
    ])
    return (try? JSONEncoder().encode(answer)) ?? Data()
}

/// A deck made from a prompt (PLAN G.6), as New from a Prompt makes one: a file read, the arc and
/// the slides written on Anthropic's recorded answers, and the slides' pictures made by a stand-in
/// for OpenAI's images API (PLAN G.10). The model is shown each slide as it is drawn and tightens
/// the cover's title (PLAN G.13). It opens with a state for each slide and each picture among its
/// files, and each key went to its own provider alone.
@MainActor
@Test func aDeckIsMadeFromAPromptWithItsPictures() async throws {
    let arc = try Data(contentsOf: answers.appending(path: "anthropic-arc.json"))
    let slides = try Data(contentsOf: answers.appending(path: "anthropic-slides.json"))
    let jpeg = try Data(contentsOf: repository.appending(path: "tests/fixtures/jpeg/baseline-422.jpg"))
    let image = try JSONEncoder().encode(
        JSONValue.object(["data": .array([.object(["b64_json": .string(jpeg.base64EncodedString())])])]))
    let models = Data(#"{"data": [{"id": "gpt-4o"}, {"id": "dall-e-3"}, {"id": "gpt-image-1"}]}"#.utf8)
    let asked = Asked()
    let generator = Generator { request in
        await asked.add(request)
        switch request.url {
        case "https://api.openai.com/v1/models":
            return GeneratorReply(status: 200, body: models)
        case "https://api.openai.com/v1/images/generations":
            return GeneratorReply(status: 200, body: image)
        default:
            let body = request.body ?? ""
            if body.contains("=== CATALOG ===") { return GeneratorReply(status: 200, body: arc) }
            if body.contains("runs past its place") {
                return GeneratorReply(status: 200, body: anthropic(#"{"texts": []}"#))
            }
            if body.contains("art director") {
                let look = #"{"slides": [{"slide": 1, "layout": "", "title": "Carbon-negative cargo, ready to scale", "subtitle": "", "bullets": [], "why": "One idea, one line."}]}"#
                return GeneratorReply(status: 200, body: anthropic(look))
            }
            return GeneratorReply(status: 200, body: slides)
        }
    }
    let notes = """
        # Pilots

        Three lanes ran carbon-negative for a quarter. Fuel cost rose 4%, and every shipper renewed its \
        contract for the next season.
        """
    let read = try generator.add(name: "notes.md", data: Data(notes.utf8))
    #expect(read == .read(name: "notes.md", kind: "markdown"))

    // The deck built and every slide drawn away from the main actor (lift-up narrative PERF-3).
    let away = Away()
    generator.away = { away.add($0, main: Thread.isMainThread, stack: Thread.current.stackSize) }

    var heard: [GeneratingStep] = []
    let (session, made) = try await generator.generate(
        brief: Brief(topic: "Carbon-negative cargo is ready to scale", audience: "Investors"),
        provider: .anthropic, model: "a-model", key: "sk-writer", theme: "dusk",
        painter: Generator.Painter(provider: .openai, key: "sk-pictures")
    ) { heard.append($0) }

    #expect(made.created.created && made.created.errors == 0)
    #expect(made.arc == "Problem–Solution")
    #expect(try session.states().count == made.layouts.count)
    let pictures = made.pictures ?? 0
    #expect(pictures > 0)
    let kept = try session.bundleFiles().map(\.path).filter { $0.hasPrefix("assets/picture-") }
    #expect(kept.count == pictures)
    #expect(kept.allSatisfy { $0.hasSuffix(".jpg") })
    #expect(heard.first == .choosing)
    #expect(heard.contains(.painting(pictures: pictures, model: "gpt-image-1")))
    #expect(heard.contains(.painted(pictures: pictures, unmade: [])))
    #expect(heard.contains(.making))
    #expect(heard.contains(.looking(slides: made.layouts.count)))
    #expect(heard.contains(.looked(slides: 1)))
    #expect(made.looked?.first?.slide == 1)
    let looked = await asked.requests.filter { ($0.body ?? "").contains("art director") }
    #expect(looked.count == 1)
    #expect((looked.first?.body ?? "").components(separatedBy: "image/jpeg").count - 1 == made.layouts.count)
    #expect(made.words.contains("\(pictures) picture"))
    // Built and drawn, then drawn again as the model's look changed it; fitted where a text ran on.
    let slow = away.steps.map(\.step)
    #expect(Array(slow.prefix(2)) == ["make", "looked"], "the slow steps: \(slow)")
    #expect(Set(slow).isSubset(of: ["make", "looked", "fitted"]), "the slow steps: \(slow)")
    #expect(away.steps.allSatisfy { !$0.main }, "a slow step was made on the main thread: \(away.steps)")
    // On a stack that holds a deck being built: a dispatch queue's 512 KB worker overflows it.
    #expect(away.steps.allSatisfy { $0.stack >= Generator.stack }, "a slow step's stack: \(away.steps)")

    // Its story (PLAN G.9), as the Story panel reads it: the arc it was written to, each slide what it
    // declares, coherence by words unscored; a fix made, and the finding gone.
    let health = try session.health()
    #expect(health.arc == "Problem–Solution")
    #expect(health.slides.count == made.layouts.count)
    #expect(health.slides.allSatisfy { $0.declared != nil })
    #expect(health.coherence.score == nil)
    if let fixable = health.findings.first(where: { $0.fix != nil }), let fix = fixable.fix {
        let applied = try session.tool("deck_patch", ["ops": .array(fix.patch)])
        #expect(applied.edited)
        #expect(try !session.health().findings.contains { $0.state == fixable.state && $0.kind == fixable.kind })
    }

    let requests = await asked.requests
    #expect(requests.filter { $0.url == "https://api.openai.com/v1/images/generations" }.count == pictures)
    for request in requests {
        let body = request.body ?? ""
        #expect(!body.contains("sk-writer") && !body.contains("sk-pictures"), "no key in a body")
        if request.url.hasPrefix("https://api.openai.com/") {
            #expect(request.headers["authorization"] == "Bearer sk-pictures")
            #expect(request.headers["x-api-key"] == nil)
        } else {
            #expect(request.headers["x-api-key"] == "sk-writer")
            #expect(request.headers["authorization"] == nil)
        }
    }
}

/// The outline before the slides (narrative FUNC-1): asked once the arc is chosen and shown to
/// settle; moved, rewritten, cut, and added to, it goes into the slides' prompt as Scaena's block,
/// and the deck's sections follow its beats. Without it asked for, the slides come at once.
@MainActor
@Test func theOutlineIsShownToSettleBeforeTheSlides() async throws {
    let arc = try Data(contentsOf: answers.appending(path: "anthropic-arc.json"))
    let slides = try Data(contentsOf: answers.appending(path: "anthropic-slides.json"))
    let outline = #"{"bigIdea": "Cargo can pay for its carbon.", "beats": [{"beat": 1, "claim": "Shipping's carbon bill is due."}, {"beat": 2, "claim": "Carriers cannot cut it with today's ships."}, {"beat": 3, "claim": "The fixes tried so far fall short."}, {"beat": 4, "claim": "Tidewater's lanes run carbon-negative."}, {"beat": 5, "claim": "Shippers pay more for them."}, {"beat": 6, "claim": "Join the round."}]}"#
    let asked = Asked()
    let generator = Generator { request in
        await asked.add(request)
        let body = request.body ?? ""
        if body.contains("=== CATALOG ===") { return GeneratorReply(status: 200, body: arc) }
        if body.contains("planning a presentation before its slides") { return GeneratorReply(status: 200, body: anthropic(outline)) }
        if body.contains("runs past its place") { return GeneratorReply(status: 200, body: anthropic(#"{"texts": []}"#)) }
        if body.contains("art director") { return GeneratorReply(status: 200, body: anthropic(#"{"slides": []}"#)) }
        return GeneratorReply(status: 200, body: slides)
    }
    var heard: [GeneratingStep] = []
    var shown: Outlined?
    let (session, made) = try await generator.generate(
        brief: Brief(topic: "Carbon-negative cargo is ready to scale", audience: "Investors"),
        provider: .anthropic, model: "a-model", key: "sk-writer", theme: "dusk", look: false,
        outline: { outlined in
            shown = outlined
            // The proof moved before the mechanism, the second beat's claim rewritten, the last
            // dropped, and a beat of the person's own added.
            var settled = outlined.outline
            let proof = settled.beats.remove(at: 4)
            settled.beats.insert(proof, at: 3)
            settled.beats[1].claim = "No carrier can cut it alone."
            settled.beats.removeLast()
            settled.beats.append(.init(name: "Thanks", claim: "Thank you for listening."))
            return .settled(settled)
        }
    ) { heard.append($0) }
    #expect(shown?.arc == "Problem–Solution")
    #expect(shown?.outline.bigIdea == "Cargo can pay for its carbon.")
    #expect(shown?.outline.beats.count == 6)
    #expect(shown?.outline.beats[0].claim == "Shipping's carbon bill is due.")
    #expect(heard.contains(.outlining))
    #expect(heard.contains(.outlined(beats: 6)))
    #expect(heard.contains(.settled(beats: 6)))
    #expect(made.created.created)
    let requests = await asked.requests.filter { $0.url.hasSuffix("/v1/messages") }
    #expect(requests.count == 3, "the arc, the outline, then the slides")
    let written = requests[2].body ?? ""
    #expect(written.contains("**Outline (Scaena)**"))
    #expect(written.contains("No carrier can cut it alone."))
    #expect(written.contains("Beat 6 \\\"Thanks\\\""), "\(written)")
    #expect(!written.contains("Join the round."), "the beat dropped")
    // The deck keeps the outline, and its first section is the outline's first beat.
    let deck = try session.tool("deck_read", [:])
    #expect(deck.result["deck"]?["meta"]?["narrative"]?["outline"]?["bigIdea"]?.string == "Cargo can pay for its carbon.")
    #expect(deck.result["deck"]?["spine"]?["sections"]?[0]?["title"]?.string == "Problem")

    // Slides at once, with no outline asked for: two calls.
    let plain = Asked()
    let direct = Generator { request in
        await plain.add(request)
        let body = request.body ?? ""
        if body.contains("=== CATALOG ===") { return GeneratorReply(status: 200, body: arc) }
        if body.contains("runs past its place") { return GeneratorReply(status: 200, body: anthropic(#"{"texts": []}"#)) }
        return GeneratorReply(status: 200, body: slides)
    }
    _ = try await direct.generate(
        brief: Brief(topic: "Carbon-negative cargo is ready to scale"), provider: .anthropic, model: "a-model",
        key: "sk-writer", theme: "dusk", look: false
    ) { _ in }
    let direct2 = await plain.requests.filter { $0.url.hasSuffix("/v1/messages") }
    #expect(direct2.count == 2)
    #expect(!(direct2[1].body ?? "").contains("**Outline (Scaena)**"))
}

/// A provider that refuses says why in a person's words, as the pictures do (lift-up USE-3), its
/// own words left to the log, and no deck is made.
@MainActor
@Test func aProvidersRefusalIsSaidInItsWords() async throws {
    let generator = Generator { _ in
        GeneratorReply(
            status: 401, body: Data(#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#.utf8))
    }
    let refused = await #expect(throws: ScaenaError.self) {
        _ = try await generator.generate(
            brief: Brief(topic: "Anything"), provider: .anthropic, model: "a-model", key: "sk-wrong", theme: "dusk"
        ) { _ in }
    }
    #expect(refused?.message == "Anthropic didn't accept your key. Check it, or replace it.", "\(String(describing: refused))")
}

/// A pictures key whose image models cannot be listed loses the pictures, not the deck
/// (narrative REL-4): the slides written are made without them, as the browser makes them, and
/// why is heard in a person's words, never the provider's.
@MainActor
@Test func aPicturesKeyThatListsNoModelLosesThePicturesNotTheDeck() async throws {
    let arc = try Data(contentsOf: answers.appending(path: "anthropic-arc.json"))
    let slides = try Data(contentsOf: answers.appending(path: "anthropic-slides.json"))
    let asked = Asked()
    let generator = Generator { request in
        await asked.add(request)
        switch request.url {
        case "https://api.openai.com/v1/models":
            let refused = #"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error","code":"invalid_api_key"}}"#
            return GeneratorReply(status: 401, body: Data(refused.utf8))
        default:
            let body = request.body ?? ""
            if body.contains("=== CATALOG ===") { return GeneratorReply(status: 200, body: arc) }
            if body.contains("runs past its place") {
                return GeneratorReply(status: 200, body: anthropic(#"{"texts": []}"#))
            }
            if body.contains("art director") { return GeneratorReply(status: 200, body: anthropic(#"{"slides": []}"#)) }
            return GeneratorReply(status: 200, body: slides)
        }
    }
    var heard: [GeneratingStep] = []
    let (session, made) = try await generator.generate(
        brief: Brief(topic: "Carbon-negative cargo is ready to scale", audience: "Investors"),
        provider: .anthropic, model: "a-model", key: "sk-writer", theme: "dusk",
        painter: Generator.Painter(provider: .openai, key: "sk-wrong")
    ) { heard.append($0) }
    #expect(made.created.created && made.created.errors == 0)
    #expect(try session.states().count == made.layouts.count)
    #expect((made.pictures ?? 0) == 0)
    let unmade = heard.compactMap { step -> [String]? in
        if case .painted(let pictures, let unmade) = step, pictures == 0 { return unmade }
        return nil
    }
    #expect(unmade == [["OpenAI didn't accept your key. Check it, or replace it."]], "\(heard)")
    let requests = await asked.requests
    #expect(requests.filter { $0.url == "https://api.openai.com/v1/images/generations" }.isEmpty)
    #expect(!requests.contains { ($0.body ?? "").contains("Incorrect API key") })
}

/// A deck in a script the theme's fonts do not draw takes the script's font, fetched, held to its
/// digest, and kept before any provider is asked anything (PLAN G.22). Kept, it is not fetched for
/// the next deck. A font that cannot be fetched stops the deck there, in a person's words, and a
/// deck in Ukrainian needs none.
@MainActor
@Test func aDeckInAnotherScriptTakesItsFontFirst() async throws {
    let devanagari = try Data(contentsOf: repository.appending(path: "tests/fixtures/scripts/NotoSansDevanagari-VF.ttf"))
    let kept = FileManager.default.temporaryDirectory.appending(path: "scaena-fonts-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: kept) }
    let asked = Asked()
    let fonts: Generator.Send = { request in
        await asked.add(request)
        if request.url.contains("NotoSansDevanagari") { return GeneratorReply(status: 200, body: devanagari) }
        return GeneratorReply(status: 404, body: Data())
    }
    var heard: [String] = []
    try await Generator(send: fonts, fonts: kept).prepare(language: "Hindi", theme: "dusk") { heard.append($0.words) }
    #expect(heard == ["Fetching Noto Sans Devanagari, the font a deck in Hindi is set in (0.6 MB)…"])
    #expect(await asked.requests.map(\.method) == ["GET"])
    #expect(await asked.requests.first?.url.hasPrefix("https://raw.githubusercontent.com/google/fonts/") == true)

    heard = []
    try await Generator(send: fonts, fonts: kept).prepare(language: "Hindi", theme: "ember") { heard.append($0.words) }
    #expect(heard.isEmpty, "kept, it is not fetched again")
    #expect(await asked.requests.count == 1)

    let refused = await #expect(throws: ScaenaError.self) {
        try await Generator(send: fonts, fonts: kept).prepare(language: "Japanese", theme: "dusk")
    }
    #expect(refused?.message.hasPrefix("a deck in Japanese is set in Noto Sans JP, which could not be fetched") == true, "\(String(describing: refused))")
    #expect(refused?.message.contains("PLAN") == false)

    try await Generator(send: fonts, fonts: kept).prepare(language: "Ukrainian", theme: "ember")
    #expect(await asked.requests.count == 2, "Ukrainian fetches nothing")
}

/// What a brief may ask is Narrato's: its goals, lengths, arcs, and defaults.
/// A redirect is followed only to a public https address by name: a page a source names cannot
/// send the app on to this machine or its network (lift-up SEC-2).
@Test func aRedirectIsFollowedOnlyToAPublicAddress() throws {
    for followed in ["https://example.com/a", "https://www.example.org./b?c", "https://cdn.example.net:8443/x"] {
        #expect(GeneratorReply.follows(URL(string: followed)!), "\(followed)")
    }
    for refused in [
        "http://example.com/a", "https://localhost/a", "https://api.localhost/a", "https://intranet/a",
        "https://127.0.0.1/a", "https://169.254.169.254/latest", "https://0x7f.0.0.1/a", "https://[::1]/a",
        "https://[fd00::1]/a", "file:///etc/hosts",
    ] {
        #expect(!GeneratorReply.follows(URL(string: refused)!), "\(refused)")
    }
}

@Test func whatABriefMayAskIsNarratos() throws {
    let choices = try Generator.choices()
    #expect(choices.goals.contains("Inform"))
    #expect(choices.goals.contains(choices.defaults.goal))
    #expect(choices.lengths.contains(choices.defaults.length))
    #expect(choices.arcs.contains("Problem–Solution"))
    #expect(!choices.modalities.isEmpty)
    #expect(choices.themes.map(\.name) == ["dusk", "daybreak", "ember", "salon"])
}

/// A server that answers every request with `Endless.chunks` chunks of 64 KB and no length.
private final class Endless: URLProtocol {
    nonisolated(unsafe) static var chunks = 64

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let response = HTTPURLResponse(
            url: request.url!, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "text/html"])!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        for _ in 0..<Endless.chunks {
            client?.urlProtocol(self, didLoad: Data(count: 1 << 16))
        }
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

/// A source's answer is read no further than its plan's `most` (the 20261024 plan's SEC-2): one
/// that runs past it is refused as it does, saying so; one within it reads whole.
@Test func aSourcePastItsMostIsRefusedAsItIsRead() async throws {
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [Endless.self]
    let session = URLSession(configuration: configuration)
    let asked = Data(#"{"method":"GET","url":"https://example.com/page","headers":{},"most":1048576}"#.utf8)
    let request = try JSONDecoder().decode(GeneratorRequest.self, from: asked)
    Endless.chunks = 64
    let refused = await GeneratorReply.made(request, by: session)
    #expect(refused.status == 0)
    #expect(refused.text == "https://example.com/page answered with more than 1 MB", "\(refused.text.prefix(80))")
    Endless.chunks = 8
    let whole = await GeneratorReply.made(request, by: session)
    #expect(whole.status == 200 && whole.body.count == 8 << 16)
}
