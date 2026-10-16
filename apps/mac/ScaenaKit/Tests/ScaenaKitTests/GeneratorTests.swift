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

/// A provider that refuses says why, in its words, and no deck is made.
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
    #expect(refused?.message.contains("invalid x-api-key") == true, "\(String(describing: refused))")
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
@Test func whatABriefMayAskIsNarratos() throws {
    let choices = try Generator.choices()
    #expect(choices.goals.contains("Inform"))
    #expect(choices.goals.contains(choices.defaults.goal))
    #expect(choices.lengths.contains(choices.defaults.length))
    #expect(choices.arcs.contains("Problem–Solution"))
    #expect(!choices.modalities.isEmpty)
    #expect(choices.themes.map(\.name) == ["dusk", "daybreak", "ember", "salon"])
}
