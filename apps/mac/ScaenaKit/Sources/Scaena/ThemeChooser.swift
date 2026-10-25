import ImageIO
import ScaenaKit
import SwiftUI

/// The themes that ship, offered as a new deck's window opens (PLAN 3.3), as the browser's New
/// offers them (PLAN 2.12) and a presentation app's theme chooser does: each drawn as a title
/// slide started in it, by its name, the first selected. A click selects one, and Create, Return,
/// or a double click puts the deck in it before anything is on it (lift-up ATTR-5); Cancel, or
/// Escape, keeps the theme it was made in.
struct ThemeChooser: View {
    /// The themes that ship.
    let themes: [ShippedTheme]
    /// The name of the theme chosen; none to keep the deck's.
    let chosen: (String?) -> Void
    @State private var drawn: [String: CGImage] = [:]
    @State private var selected: String?
    /// The chooser holds the focus while it shows, so Return and Escape come to it, never to a
    /// canvas or a source pane under it (lift-up ATTR-5).
    @FocusState private var focused: Bool
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
            // By family, each family a section (PLAN T.0d).
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: Self.width), spacing: 16)], alignment: .leading, spacing: 16
            ) {
                ForEach(themes.byFamily) { family in
                    Section {
                        ForEach(family.themes) { theme in tile(theme) }
                    } header: {
                        Text(family.name)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            HStack {
                Spacer()
                Button("Cancel") { chosen(nil) }
                    .keyboardShortcut(.cancelAction)
                Button("Create") { chosen(selected) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(selected == nil)
                    .accessibilityIdentifier("theme-create")
            }
        }
        .padding(24)
        .frame(minWidth: 480, idealWidth: 760)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.return) {
            guard let selected else { return .ignored }
            chosen(selected)
            return .handled
        }
        .onKeyPress(.escape) {
            chosen(nil)
            return .handled
        }
        .onAppear { focused = true }
        // Drawn again, and the first selected, once the themes are given (they may come after the
        // chooser shows).
        .task(id: themes.map(\.name)) {
            if selected == nil { selected = themes.byFamily.first?.themes.first?.name }
            await draw()
        }
    }

    private func tile(_ theme: ShippedTheme) -> some View {
        let name = Words.phrase(theme.name)
        let isSelected = selected == theme.name
        return Button {
            selected = theme.name
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
                .padding(3)
                .overlay(
                    RoundedRectangle(cornerRadius: 9)
                        .strokeBorder(isSelected ? Color.accentColor : .clear, lineWidth: 3)
                )
                Text(name)
                    .font(.headline)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // A double click, or a double tap, creates the deck in it, as a presentation app's does.
        .simultaneousGesture(TapGesture(count: 2).onEnded { chosen(theme.name) })
        .help("Start the deck in \(name)")
        .accessibilityLabel(name)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityIdentifier("theme-\(theme.name)")
    }

    /// Each theme's title slide: the one the Mac app's build drew, else painted away from the
    /// window, one at a time, the chooser shown first and each tile as it lands (lift-up PERF-4).
    private func draw() async {
        for theme in themes where drawn[theme.name] == nil {
            if let built = Self.built(theme.name) {
                drawn[theme.name] = built
                continue
            }
            guard !Task.isCancelled else { return }
            drawn[theme.name] = await Self.titleSlide(in: theme.name, width: Int(Self.width * scale))
        }
    }

    /// The title slide of the theme that ships named `name` as the Mac app's build drew it, by
    /// `scaena-cli`'s `theme_thumbnails` into its Resources' `themes` folder, as the browser's
    /// build draws its New dialog's (PLAN T.0c, lift-up PERF-4); none on the iPad, whose build
    /// draws none, or where the app was built without them, as `swift run` builds it.
    private static func built(_ name: String) -> CGImage? {
        guard let url = Bundle.main.url(forResource: name, withExtension: "png", subdirectory: "themes"),
            let source = CGImageSourceCreateWithURL(url as CFURL, nil)
        else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    /// What the theme that ships named `name` looks like: a title slide started in a deck of its
    /// own as New starts one, its words those the theme's layout says go there and its backdrop in
    /// its picture's place, drawn `width` pixels wide, painted away from the window (lift-up
    /// PERF-4).
    private static func titleSlide(in name: String, width: Int) async -> CGImage? {
        guard let session = try? ScaenaSession.create(theme: name, title: "Untitled") else { return nil }
        let editor = DeckEditor(session: session)
        guard let first = editor.slots.first?.state,
            let started = try? session.titling(after: first),
            (try? editor.make(started.patch)) != nil
        else { return nil }
        return await editor.drawingAway(started.id, width: width)
    }
}
