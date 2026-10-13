import CoreGraphics
import Foundation
import Testing

@testable import ScaenaKit

/// What a test's image provider was asked, request by request.
private actor Painted {
    private(set) var requests: [GeneratorRequest] = []

    func add(_ request: GeneratorRequest) {
        requests.append(request)
    }
}

/// Make a Picture (PLAN 2.107), as the Mac's sheet makes one, against a stand-in for OpenAI's
/// images API. On a slide started in Photo with nothing selected, the picture fills the empty
/// place for one, asked once of what the person says it shows, framed wide as the place is, in the
/// theme's palette; it joins the bundle and says what it shows, one patch. The image selected
/// shows the next picture in its place, one patch of its `src` and `alt`. A refusal says why.
@MainActor
@Test func aPictureMadeFillsItsPlaceOrTakesThePlaceOfTheOneSelected() async throws {
    let lighthouse = try Data(contentsOf: repository.appending(path: "tests/fixtures/jpeg/baseline-422.jpg"))
    let harbor = try Data(contentsOf: repository.appending(path: "tests/fixtures/jpeg/baseline-444.jpg"))
    let models = Data(#"{"data": [{"id": "gpt-4o"}, {"id": "gpt-image-1"}]}"#.utf8)
    let refused = Data(#"{"error": {"message": "Your request was rejected by the safety system."}}"#.utf8)
    let painted = Painted()
    let generator = Generator { request in
        await painted.add(request)
        if request.url.hasSuffix("/v1/models") { return GeneratorReply(status: 200, body: models) }
        let body = request.body ?? ""
        if body.contains("SUBJECT: refuse") { return GeneratorReply(status: 400, body: refused) }
        let picture = body.contains("SUBJECT: A harbor") ? harbor : lighthouse
        let answer = JSONValue.object(["data": .array([.object(["b64_json": .string(picture.base64EncodedString())])])])
        return GeneratorReply(status: 200, body: (try? JSONEncoder().encode(answer)) ?? Data())
    }
    let editor = DeckEditor(session: try ScaenaSession.create(theme: "Dusk", title: "Pictures"))
    let session = editor.session
    let started = try session.starting(after: "start", layout: "photo")
    try editor.make(started.patch)
    let theme = try #require(try session.themeText()?.text)
    let painter = Generator.Painter(provider: .openai, key: "sk-pictures", look: "photo")

    // Nothing selected: the slide's empty place for a picture, edge to edge, framed wide.
    let place = try session.pictureTarget(state: started.id, selected: [], pressed: nil)
    #expect(place.slot == "art" && place.node == nil && place.frame == "wide")
    let made = try await generator.picture(subject: "A lighthouse at dusk", theme: theme, size: place.size, painter: painter)
    #expect(made.alt == "A lighthouse at dusk" && made.ext == "jpg" && made.model == "gpt-image-1")
    #expect(made.bytes == lighthouse, "the picture's own bytes")
    #expect(made.name == "a-lighthouse-at-dusk.jpg" && made.shows == "a lighthouse at dusk")
    let put = try session.puttingPicture(made, at: place, in: started.id)
    #expect(put.said == "Picture of a lighthouse at dusk added, made by gpt-image-1")
    try editor.make(put.patch)
    #expect(try session.waiting(state: started.id).isEmpty, "the place waits no more")
    let box = try #require(try session.boxes(state: started.id).first { $0.node == put.node })
    #expect(box.rect == [0, 0, 1920, 1080], "edge to edge, as its place is")
    let alt = { try session.choices(state: started.id, node: put.node).fields.first { $0.prop == "alt" }?.value?.string }
    #expect(try alt() == "A lighthouse at dusk")

    // The image selected: the next picture takes its place, framed as it is, saying what it shows.
    let target = try session.pictureTarget(state: started.id, selected: [put.node], pressed: nil)
    #expect(target.node == put.node && target.shows == "A lighthouse at dusk" && target.frame == "wide")
    let next = try await generator.picture(
        subject: "A harbor at night", theme: theme, size: target.size,
        painter: Generator.Painter(provider: .openai, key: "sk-pictures"))
    let replaced = try session.puttingPicture(next, at: target, in: started.id)
    #expect(replaced.node == put.node && replaced.patch.count == 2)
    #expect(replaced.said == "Picture replaced with one of a harbor at night, made by gpt-image-1")
    try editor.make(replaced.patch)
    #expect(try alt() == "A harbor at night")

    // Each asked once, wide, of the words given, the key in OpenAI's header alone.
    let asked = await painted.requests.filter { $0.url.hasSuffix("/v1/images/generations") }
    #expect(asked.count == 2)
    #expect(asked.allSatisfy { $0.headers["authorization"] == "Bearer sk-pictures" })
    #expect(asked.allSatisfy { $0.body?.contains("Landscape composition") == true })
    #expect(asked.first?.body?.contains("Editorial photography") == true, "the first as a photograph")
    #expect(asked.last?.body?.contains("editorial illustration") == true, "the next as an illustration")

    // Refused: why, and nothing to put.
    await #expect(throws: ScaenaError.self) {
        _ = try await generator.picture(subject: "refuse this", theme: theme, size: place.size, painter: painter)
    }
}
