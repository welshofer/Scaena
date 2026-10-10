import ScaenaKit
import SwiftUI

/// The themes that ship, offered as a new deck's window opens (PLAN 3.3), as the browser's New
/// offers them (PLAN 2.12) and a presentation app's theme chooser does: each drawn as a title
/// slide started in it, by its name. A click puts the deck in that theme before anything is on
/// it; Cancel, or Escape, keeps the theme it was made in.
struct ThemeChooser: View {
    /// The themes that ship.
    let themes: [ShippedTheme]
    /// The name of the theme a click chose; none to keep the deck's.
    let chosen: (String?) -> Void
    @State private var drawn: [String: CGImage] = [:]
    @Environment(\.displayScale) private var scale

    /// How wide a theme's slide is shown, points.
    private static let width: CGFloat = 220

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Choose a Theme")
                    .font(.title2.weight(.semibold))
                Text("A theme gives every slide its colors, type, and layouts. Document › Theme changes it later.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: Self.width), spacing: 16)], spacing: 16) {
                ForEach(themes) { theme in tile(theme) }
            }
            HStack {
                Spacer()
                Button("Cancel") { chosen(nil) }
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(24)
        .frame(minWidth: 480, idealWidth: 760)
        .task { await draw() }
    }

    private func tile(_ theme: ShippedTheme) -> some View {
        let name = Words.phrase(theme.name)
        return Button {
            chosen(theme.name)
        } label: {
            VStack(spacing: 6) {
                Group {
                    if let image = drawn[theme.name] {
                        Image(decorative: image, scale: scale)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                    } else {
                        Rectangle()
                            .fill(.quaternary)
                            .aspectRatio(16 / 9, contentMode: .fit)
                            .overlay { ProgressView().controlSize(.small) }
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.separator))
                Text(name)
                    .font(.headline)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Start the deck in \(name)")
        .accessibilityLabel(name)
        .accessibilityIdentifier("theme-\(theme.name)")
    }

    /// Each theme drawn, one at a time, the sheet shown first and each tile as it is drawn.
    private func draw() async {
        for theme in themes where drawn[theme.name] == nil {
            await Task.yield()
            guard !Task.isCancelled else { return }
            drawn[theme.name] = Self.titleSlide(in: theme.name, width: Int(Self.width * scale))
        }
    }

    /// What the theme that ships named `name` looks like: a title slide started in a deck of its
    /// own, its words those the theme's layout says go there, drawn `width` pixels wide.
    private static func titleSlide(in name: String, width: Int) -> CGImage? {
        guard let session = try? ScaenaSession.create(theme: name, title: "Untitled") else { return nil }
        let editor = DeckEditor(session: session)
        guard let first = editor.slots.first?.state,
            let started = try? session.starting(after: first, layout: "title"),
            (try? editor.make(started.patch)) != nil
        else { return nil }
        return editor.drawing(started.id, width: width)
    }
}
