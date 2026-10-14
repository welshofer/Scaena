import Foundation
import ScaenaKit
import Testing

/// The deck's exports (PLAN 3.8): the PDF `scaena export` writes, which the app shares, shows in
/// Quick Look, and saves; and a state at rest as a PNG the width asked.
@Test func theDeckExportsAsTheCLIWritesIt() throws {
    let session = try ScaenaSession(directory: b1)
    let pdf = try session.pdf()
    #expect(pdf.starts(with: Array("%PDF-".utf8)), "a PDF")
    let png = try session.png("cover", width: 640)
    #expect(png.starts(with: [0x89, 0x50, 0x4E, 0x47]), "a PNG")
    let width = png[16..<20].reduce(0) { $0 << 8 | Int($1) }
    #expect(width == 640)
    #expect(throws: ScaenaError.self) { try session.png("cover", width: 0) }
    #expect(throws: ScaenaError.self) { try session.png("no-such-state", width: 100) }
}

/// Every slide as PNGs in one zip, each named by its number (PLAN 2.122), as Share offers it.
@Test func everySlideExportsAsPNGsInOneZip() throws {
    let session = try ScaenaSession(directory: b1)
    let zipped = try session.slides(width: 320, name: "B1")
    #expect(zipped.starts(with: [0x50, 0x4B, 0x03, 0x04]), "a zip")
    #expect(zipped.range(of: Data("B1 - Slide 1.png".utf8)) != nil, "its first slide, by its number")
    #expect(throws: ScaenaError.self) { try session.slides(width: 0, name: "B1") }
}
