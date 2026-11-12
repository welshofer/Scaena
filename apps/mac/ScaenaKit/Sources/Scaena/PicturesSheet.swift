import ScaenaKit
import SwiftUI

/// PNGs asked for, before they are made: the slide shown's, or every slide's in one zip where
/// `state` is none, and whether they are shared or saved.
struct PicturesAsked: Identifiable {
    let id = UUID()
    let state: String?
    let sharing: Bool
}

/// A PNG's width, asked before it is made, as the browser's Export… asks it: the canvas's at first,
/// the height keeping the canvas's aspect, and what will be made said under it in the browser's
/// words. The movie's sheet asks its width in the same row (`WidthRow`).
struct PicturesSheet: View {
    let asked: PicturesAsked
    /// The canvas in the format shown, whose aspect the PNGs keep.
    let canvas: CGSize
    /// The slide shown, as a person counts it ("Slide 3, step 2"), for one PNG.
    let slide: String
    /// How many slides the deck has, for every slide's.
    let slides: Int
    /// The format shown, where it is one of the deck's.
    let format: String?
    /// Make the PNGs this many pixels wide.
    let make: (_ width: Int) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var typed: String
    @FocusState private var focused: Bool

    init(
        asked: PicturesAsked, canvas: CGSize, slide: String, slides: Int, format: String?,
        make: @escaping (_ width: Int) -> Void
    ) {
        self.asked = asked
        self.canvas = canvas
        self.slide = slide
        self.slides = slides
        self.format = format
        self.make = make
        let start = min(PNGWidth.widths.upperBound, max(PNGWidth.widths.lowerBound, Int(canvas.width.rounded())))
        _typed = State(initialValue: String(start))
    }

    /// The width typed, where it is one a PNG may be.
    private var width: Int? { PNGWidth.width(typed) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(asked.state == nil ? "Export Every Slide as PNGs" : "Export This Slide as a PNG")
                .font(.headline)
            WidthRow(typed: $typed, focused: $focused, id: "pictures-width", submit: go)
            Text(said)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("pictures-size")
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(asked.sharing ? "Share" : "Save…") { go() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(width == nil)
                    .accessibilityIdentifier("pictures-make")
            }
        }
        .padding(20)
        .frame(minWidth: 360)
        .onAppear { focused = true }
    }

    /// What will be made, as the browser's Export… says it, or what a width may be.
    private var said: String {
        guard let width else { return "A width in whole pixels: the height keeps the canvas’s aspect." }
        let made = PNGWidth.pixels(width: width, canvas: canvas)
        let pixels = "\(made.width) × \(made.height) pixels"
        let shown = format.map { " in \($0)" } ?? ""
        guard asked.state == nil else { return "\(slide)\(shown): \(pixels)." }
        let counted = "\(slides) slide\(slides == 1 ? "" : "s")"
        let inFormat = format.map { ", in \($0)" } ?? ""
        return "\(counted), each as it stands once its steps have played\(inFormat): a PNG of \(pixels) named by its "
            + "number, all in one .zip."
    }

    private func go() {
        guard let width else { return }
        make(width)
        dismiss()
    }
}
