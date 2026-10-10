import ScaenaKit
import SwiftUI

/// Add Slide's gallery, as a presentation app's (PLAN 3.30): each of the theme's layouts drawn as
/// a slide started in it, its words in its slots and a shaded box where a picture or a chart goes,
/// under the theme's sections in the order it first names each, then a blank slide. A click
/// starts that slide after the slide shown, one step to undo, and shows it; New Step on This Slide
/// is beneath.
struct SlideGallery: View {
    let editor: DeckEditor
    /// The state shown: the new slide goes after its slide.
    let state: String
    /// Start a slide in a layout, or a blank one with none.
    let start: (String?) -> Void
    /// Add a step of the slide shown.
    let step: () -> Void
    @State private var drawn: [Drawn] = []
    @State private var look: ThemeLook?

    private struct Drawn: Identifiable {
        let starter: Starter
        let image: CGImage

        var id: String { starter.id }
    }

    /// How high each is painted, pixels: a tile's height on a Retina screen.
    private static let painted = 180

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Add a Slide")
                .font(.headline)
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 14) {
                    ForEach(sections, id: \.title) { section in
                        Section {
                            ForEach(section.slides) { one in tile(one) }
                        } header: {
                            Text(section.title)
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.top, 6)
                        }
                    }
                }
                .padding(2)
            }
            .frame(minHeight: 320, idealHeight: 460)
            .overlay {
                if drawn.isEmpty { ProgressView() }
            }
            Divider()
            Button("New Step on This Slide", action: step)
                .accessibilityIdentifier("add-step")
        }
        .padding(14)
        .frame(idealWidth: 540)
        // Once the popover is up: each slide laid out and drawn, the theme's names for them read.
        .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(editor.format ?? "")") {
            try? await Task.sleep(for: .milliseconds(30))
            guard !Task.isCancelled else { return }
            look = ThemeLook(try? editor.session.themeText())
            let found = (try? editor.session.startersDrawn(after: state, height: Self.painted)) ?? []
            drawn = found.map { Drawn(starter: $0.starter, image: $0.image) }
        }
    }

    /// The slides drawn, under the theme's sections in the order it first names each, then those
    /// it puts in none, then the blank slide.
    private var sections: [(title: String, slides: [Drawn])] {
        var titles: [String] = []
        for one in drawn {
            if let group = one.starter.group, !titles.contains(group) { titles.append(group) }
        }
        var out = titles.map { title in (title: title, slides: drawn.filter { $0.starter.group == title }) }
        let other = drawn.filter { $0.starter.layout != nil && $0.starter.group == nil }
        if !other.isEmpty { out.append((title: titles.isEmpty ? "Layouts" : "Other", slides: other)) }
        let blank = drawn.filter { $0.starter.layout == nil }
        if !blank.isEmpty { out.append((title: "Blank", slides: blank)) }
        return out
    }

    private func tile(_ one: Drawn) -> some View {
        let name = one.starter.layout.map { Words.name($0, of: "layout", theme: look) } ?? "Blank"
        return Button {
            start(one.starter.layout)
        } label: {
            VStack(spacing: 4) {
                Image(decorative: one.image, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                    .overlay {
                        RoundedRectangle(cornerRadius: 4).strokeBorder(Color.secondary.opacity(0.3), lineWidth: 1)
                    }
                Text(name)
                    .font(.caption)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(one.starter.description ?? name)
        .accessibilityLabel(name)
        .accessibilityHint(one.starter.description ?? "")
        .accessibilityIdentifier("start-\(one.starter.layout ?? "blank")")
    }
}
