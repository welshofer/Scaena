import ScaenaKit
import SwiftUI

/// A movie asked for, before it is made: the sound under it, if one was chosen, and whether it is
/// shared or saved.
struct MovieAsked: Identifiable {
    let id = UUID()
    let sound: String?
    let sharing: Bool
}

/// The movie's size and pace, asked before it is made, as the browser's Export… asks them
/// (lift-up 20261031 FUNC-2): its width in pixels, the canvas's at first, the height following the
/// slide's proportions, and its frames a second, 24, 30, or 60.
struct MovieSheet: View {
    let asked: MovieAsked
    /// The deck's canvas, whose proportions the movie keeps.
    let canvas: CGSize
    /// Make the movie this many pixels wide, at this many frames a second.
    let make: (_ width: Int, _ fps: Int) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var typed: String
    @State private var fps = MovieSize.fps
    @FocusState private var focused: Bool

    init(asked: MovieAsked, canvas: CGSize, make: @escaping (_ width: Int, _ fps: Int) -> Void) {
        self.asked = asked
        self.canvas = canvas
        self.make = make
        _typed = State(initialValue: String(min(MovieSize.widths.upperBound, max(MovieSize.widths.lowerBound, Int(canvas.width)))))
    }

    /// The width typed, where it is one a movie may be.
    private var width: Int? { MovieSize.width(typed) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(asked.sound.map { "Export a Movie with \(Exporting.soundName($0))" } ?? "Export a Movie")
                .font(.headline)
            WidthRow(typed: $typed, focused: $focused, id: "movie-width", submit: go)
            Picker("Frames a Second", selection: $fps) {
                ForEach(MovieSize.rates, id: \.self) { rate in Text("\(rate)").tag(rate) }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("movie-fps")
            Text(said)
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("movie-size")
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(asked.sharing ? "Share" : "Save…") { go() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(width == nil)
                    .accessibilityIdentifier("movie-make")
            }
        }
        .padding(20)
        .frame(minWidth: 360)
        .onAppear { focused = true }
    }

    /// What the movie will be, in pixels, or what a width may be.
    private var said: String {
        guard let width else {
            return "A movie is \(MovieSize.widths.lowerBound) to \(MovieSize.widths.upperBound) pixels wide."
        }
        let made = MovieSize.pixels(width: width, canvas: canvas)
        return "\(made.width) × \(made.height) pixels, the slide's proportions, at \(fps) frames a second."
    }

    private func go() {
        guard let width else { return }
        make(width, fps)
        dismiss()
    }
}
